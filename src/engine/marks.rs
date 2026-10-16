//! Marks from the browser (`crate::browser`, the `extension/` folder): what the person marked on a
//! web page, with what they want, becomes a card. A new agent starts on it, set up the way the
//! launch pad would set it up (the person's last agent, model, reasoning, addenda, folder), or an
//! agent on the board is asked: at once when it is free, else as a queued prompt card linked after
//! what it still has to do (the bar does the same, see `queueStep` in 90-omnibar.js).

use super::*;

/// The size of a card nobody resized, and how far its left port reaches (`NW`, `NH`, `JX` in 00-base.js).
const CARD_W: f64 = 600.0;
const CARD_H: f64 = 260.0;
const PORT_X: f64 = 82.0;

impl Engine {
    pub(super) fn cmd_mark(&mut self, text: String, to: String, agent: String, cwd: Option<String>, draft: bool) -> Value {
        if text.trim().is_empty() {
            return json!({"ok": false, "error": "say what is wrong or what you want"});
        }
        // (the person's choices may have changed on another board since this one read them)
        self.sync_user_config();
        let r = if to.trim().is_empty() { self.mark_agent(text, agent, cwd, draft) } else { self.mark_to(to.trim(), text) };
        if let Some(id) = r["id"].as_str() {
            let card = self.node(id).map(|n| format!("{id} · {}", n.name)).unwrap_or_else(|| id.to_string());
            self.toast("info", &format!("{card}: marked in the browser"));
            self.bcast(&json!({"t": "reveal", "id": id}));
        }
        r
    }

    /// A new agent for the mark.
    fn mark_agent(&mut self, text: String, agent: String, cwd: Option<String>, draft: bool) -> Value {
        let prefs = self.cfg.prefs.clone();
        let usable = |a: &str| self.cfg.agents.contains_key(a) && a != "sh";
        let agent = [agent.as_str(), prefs["agent"].as_str().unwrap_or(""), "claude"]
            .into_iter()
            .find(|a| usable(a))
            .map(str::to_string)
            .or_else(|| self.cfg.agents.keys().find(|a| usable(a)).cloned());
        let Some(agent) = agent else { return json!({"ok": false, "error": "no agent is set up on this board"}) };
        let ac = &self.cfg.agents[&agent];
        let tuned = |key: &str| prefs[key][agent.as_str()].as_str().unwrap_or("").to_string();
        let mut node = json!({"agent": agent, "prompt": text, "draft": draft, "addenda": self.chosen_addenda()});
        if !ac.model_arg.is_empty() {
            node["model"] = json!(tuned("model"));
        }
        if !ac.effort_arg.is_empty() {
            node["effort"] = json!(tuned("effort"));
        }
        if let (false, Some(y)) = (ac.yolo_arg.is_empty(), prefs["yolo"].as_bool()) {
            node["yolo"] = json!(y);
        }
        // where it works: where the marked page's server runs, else the folder new agents start in
        let dir = match cwd.filter(|c| !c.trim().is_empty()) {
            Some(c) => Some(self.abs_dir(&c)),
            None => prefs["here"][self.boot.dir.to_string_lossy().as_ref()].as_str().map(|d| self.abs_dir(d)),
        };
        if let Some(d) = dir.filter(|d| *d != self.boot.dir && d.is_dir()) {
            node["cwd"] = json!(d.to_string_lossy());
        }
        // beside the card made last, like a prompt from the bar
        let (x, y) = match self.flow.nodes.last() {
            Some(n) => (n.x + card_w(n) + 150.0, n.y),
            None => (0.0, 0.0),
        };
        let (x, y) = self.free_spot(x, y);
        node["x"] = json!(x);
        node["y"] = json!(y);
        self.cmd_node_add(node, None, !draft)
    }

    /// Ask an agent on the board: now when it is free, else as a card queued after its work.
    fn mark_to(&mut self, to: &str, text: String) -> Value {
        let Some(host) = self.node(to).filter(|n| n.kind() == Kind::Agent && n.agent != "sh").cloned() else {
            return json!({"ok": false, "error": format!("{to} is not an agent on this board (any more)")});
        };
        let status = self.eff_status(&host.id, &self.blocked_map());
        if !matches!(status, Status::Busy | Status::Starting) {
            let r = self.cmd_send(host.id.clone(), text, SendMode::Auto, true);
            return if r["ok"] == json!(false) { r } else { json!({"ok": true, "id": host.id, "typed": true}) };
        }
        // the end of what it still has to do: its queued prompts, one linked after the other
        let mut tail = host.id.clone();
        let mut seen = vec![host.id.clone()];
        loop {
            let blocked = self.blocked_map();
            let next = self
                .flow
                .edges
                .iter()
                .filter(|e| e.from == tail)
                .filter_map(|e| self.node(&e.to))
                .find(|n| {
                    n.kind() == Kind::Step
                        && n.host == host.id
                        && !seen.contains(&n.id)
                        && !matches!(self.eff_status(&n.id, &blocked), Status::Done | Status::Failed)
                })
                .map(|n| n.id.clone());
            match next {
                Some(id) => {
                    seen.push(id.clone());
                    tail = id;
                }
                None => break,
            }
        }
        let (x, y) = match self.node(&tail) {
            Some(t) => self.free_spot(t.x + card_w(t) + 150.0, t.y),
            None => self.free_spot(0.0, 0.0),
        };
        let node = json!({"agent": "step", "host": host.id, "x": x, "y": y, "draft": true, "prompt": text, "addenda": host.addenda});
        let r = self.cmd_node_add(node, None, false);
        let Some(id) = r["id"].as_str().map(str::to_string) else { return r };
        if let Err(e) = self.add_edge(&tail, &id, On::Done) {
            return json!({"ok": false, "error": e, "id": id});
        }
        self.cmd_arm(id.clone());
        json!({"ok": true, "id": id, "queued": true, "host": host.id})
    }

    /// The addenda the launch pad has switched on: the person's own choice, else each one's default.
    fn chosen_addenda(&self) -> Vec<String> {
        let mine = &self.cfg.prefs["addenda"];
        self.cfg
            .library
            .iter()
            .filter(|s| s.kind == SnippetKind::Addendum && mine[s.id.as_str()].as_bool().unwrap_or(s.default_on))
            .map(|s| s.id.clone())
            .collect()
    }

    /// The nearest place to (x, y) where a new card overlaps no other (`freeSpot` in 70-menus.js):
    /// pushed just past an edge of a card in the way.
    pub(super) fn free_spot(&self, x: f64, y: f64) -> (f64, f64) {
        let boxes: Vec<(f64, f64, f64, f64)> =
            self.flow.nodes.iter().map(|n| (n.x - PORT_X - 20.0, n.y - 14.0, card_w(n) + PORT_X + 40.0, card_h(n) + 28.0)).collect();
        let overlaps = |cx: f64, cy: f64| {
            boxes.iter().any(|&(bx, by, bw, bh)| cx - PORT_X < bx + bw && cx + CARD_W > bx && cy < by + bh && cy + CARD_H > by)
        };
        let snap = |v: f64| (v / 10.0).round() * 10.0;
        let (x, y) = (snap(x), snap(y));
        if !overlaps(x, y) {
            return (x, y);
        }
        let (mut xs, mut ys) = (vec![x], vec![y]);
        for &(bx, by, bw, bh) in &boxes {
            xs.push(((bx + bw + PORT_X) / 10.0).ceil() * 10.0);
            xs.push(((bx - CARD_W) / 10.0).floor() * 10.0);
            ys.push(((by + bh) / 10.0).ceil() * 10.0);
            ys.push(((by - CARD_H) / 10.0).floor() * 10.0);
        }
        let mut cands: Vec<(f64, f64, f64)> =
            xs.iter().flat_map(|&cx| ys.iter().map(move |&cy| (cx, cy, (cx - x).powi(2) + (cy - y).powi(2)))).collect();
        cands.sort_by(|a, b| a.2.total_cmp(&b.2));
        match cands.into_iter().find(|&(cx, cy, _)| !overlaps(cx, cy)) {
            Some((cx, cy, _)) => (cx, cy),
            None => (x, snap(boxes.iter().map(|b| b.1 + b.3).fold(y, f64::max))),
        }
    }
}

fn card_w(n: &Node) -> f64 {
    if n.w > 0.0 {
        n.w
    } else {
        CARD_W
    }
}

fn card_h(n: &Node) -> f64 {
    if n.h > 0.0 {
        n.h
    } else {
        CARD_H
    }
}
