//! "For each": a link that shares out the jobs its card's answer ends with (its last numbered list),
//! one job to each copy of the card it leads to. The copies are ordinary cards made by weft when the
//! card takes up the list, with its prompt, agent and settings; they link on where it does, so what
//! comes after them waits for all of them (the AND ring). A commit, a check on the link or a prompt
//! queued on the agent belongs to one agent's work, so each copy gets its own and they meet at the
//! next card that is none of these (a push, an agent that gathers). At most `each_max` cards take a
//! list: a longer one is shared out among them.

use super::*;
use std::collections::HashSet;
use std::ops::Range;

/// The jobs an answer ends with: the items of its last numbered list with two or more items (else of
/// its last such bulleted list), each with the lines that belong to it. None: nothing to share out.
pub(super) fn items(text: &str) -> Vec<String> {
    struct List {
        numbered: bool,
        indent: usize,
        items: Vec<String>,
    }
    let mut lists: Vec<List> = Vec::new();
    let mut cur: Option<List> = None;
    let (mut blank, mut fence) = (false, false);
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let fenced = fence || line.trim_start().starts_with("```");
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        let opens = if fenced { None } else { opens(line) };
        match (opens, cur.as_mut()) {
            // deeper than the list: part of its last item (a list inside it)
            (Some(_), Some(l)) if indent > l.indent + 1 => more(l.items.last_mut(), line, l.indent, blank),
            (Some((numbered, rest)), Some(l)) if numbered == l.numbered && indent + 1 >= l.indent => l.items.push(rest.to_string()),
            (Some((numbered, rest)), _) => {
                lists.extend(cur.take());
                cur = Some(List { numbered, indent, items: vec![rest.to_string()] });
            }
            (None, _) if line.trim().is_empty() => {
                blank = true;
                continue;
            }
            // indented under the item, or straight after it: it goes on
            (None, Some(l)) if indent > l.indent || fenced || !blank => more(l.items.last_mut(), line, l.indent, blank),
            (None, _) => lists.extend(cur.take()),
        }
        blank = false;
    }
    lists.extend(cur);
    let last = |numbered: bool| lists.iter().rev().find(|l| l.numbered == numbered && l.items.len() >= 2);
    last(true).or_else(|| last(false)).map(|l| l.items.iter().map(|i| i.trim().to_string()).collect()).unwrap_or_default()
}

/// A line that opens a list item (`1.`, `2)`, `-`, `*`, `•`): numbered or not, and its text.
fn opens(line: &str) -> Option<(bool, &str)> {
    let t = line.trim_start();
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    let (numbered, rest) = if (1..=3).contains(&digits) {
        (true, t[digits..].strip_prefix(['.', ')'])?)
    } else {
        (false, ["- ", "* ", "• "].iter().find_map(|m| t.strip_prefix(m)).map(|r| r.trim_start_matches(' '))?)
    };
    if numbered && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim();
    // (a to-do list's boxes are not part of the job)
    let rest = ["[ ] ", "[x] ", "[X] "].iter().find_map(|b| rest.strip_prefix(b)).unwrap_or(rest);
    (!rest.is_empty()).then_some((numbered, rest))
}

/// A line that belongs to the item before it, indented as it was under the item's text.
fn more(item: Option<&mut String>, line: &str, indent: usize, blank: bool) {
    let Some(item) = item else { return };
    let cut = line.len() - line.trim_start().len();
    let keep = cut.saturating_sub(indent + 3);
    item.push_str(if blank { "\n\n" } else { "\n" });
    item.push_str(&" ".repeat(keep));
    item.push_str(line.trim());
}

/// A card a "for each" link can copy: an agent that reads what it is told (not a command, a queued
/// prompt or a check).
fn takes_jobs(n: &Node) -> bool {
    n.kind() == Kind::Agent && n.finish == Finish::Idle && n.agent != "sh"
}

/// `n` jobs for at most `max` cards: runs of the list in its order, the first ones a job longer when
/// they do not come out even.
pub(super) fn shares(n: usize, max: usize) -> Vec<Range<usize>> {
    let k = n.min(max.max(1));
    if k == 0 {
        return Vec::new();
    }
    let (base, extra) = (n / k, n % k);
    let mut at = 0;
    (0..k)
        .map(|i| {
            let r = at..at + base + usize::from(i < extra);
            at = r.end;
            r
        })
        .collect()
}

impl Engine {
    pub(super) fn each_max(&self) -> usize {
        self.cfg.each_max.max(1)
    }

    /// Does what `id` answers go down a "for each" link (also through a check on the way)? Then it is
    /// asked to end its answer with the jobs.
    pub(super) fn lists_jobs(&self, id: &str) -> bool {
        let mut seen = HashSet::new();
        let mut todo = vec![id.to_string()];
        while let Some(at) = todo.pop() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for e in self.flow.edges.iter().filter(|e| e.from == at) {
                if e.each {
                    return true;
                }
                if self.node(&e.to).is_some_and(|n| n.kind().is_gate()) {
                    todo.push(e.to.clone());
                }
            }
        }
        false
    }

    /// A link into a copy behind a "for each" link: nothing goes down it by itself, the copy is
    /// given its share when the card it copies takes the list up.
    pub(super) fn held_for_share(&self, e: &Edge) -> bool {
        e.each && self.node(&e.to).is_some_and(|n| !n.each_of.is_empty() && self.node(&n.each_of).is_some())
    }

    /// The card takes up what came in: if a "for each" link brought a list, it keeps the first share
    /// and each other share goes to a copy of it, which starts with it at once. Returns what the card
    /// itself takes.
    pub(super) fn share_out(&mut self, id: &str, node: &Node, mut taken: Vec<(String, Input)>) -> Vec<(String, Input)> {
        if !node.each_of.is_empty() || !takes_jobs(node) {
            return taken;
        }
        let found = taken.iter().enumerate().find_map(|(i, (eid, inp))| {
            let e = self.flow.edges.iter().find(|e| e.id == *eid && e.each)?;
            (inp.kind == InputKind::Result).then(|| (i, e.clone()))
        });
        let Some((pos, link)) = found else { return taken };
        let whole = taken[pos].1.clone();
        let list = items(&whole.text);
        if list.is_empty() {
            self.toast("info", &format!("{} listed no jobs to share out, so {} takes its whole answer.", whole.name, node.name));
            return taken;
        }
        let shares = shares(list.len(), self.each_max());
        let lanes = self.lanes(id, &link, shares.len());
        let context = self.piped(&whole.from, whole.text.trim());
        let part = |k: usize| Input { kind: InputKind::Part, text: words::part(&list, shares[k].clone(), &context), ..whole.clone() };
        for (k, lane) in lanes.iter().enumerate().skip(1) {
            let Some(eid) = self.flow.edges.iter().find(|e| e.from == link.from && e.to == *lane).map(|e| e.id.clone()) else { continue };
            // (what else the card was given goes to its copies too)
            let mut theirs: Vec<(String, Input)> = taken.iter().enumerate().filter(|(i, _)| *i != pos).map(|(_, x)| x.clone()).collect();
            theirs.insert(pos.min(theirs.len()), (eid, part(k)));
            if let Some(r) = self.rt.get_mut(lane) {
                r.inputs = theirs;
                r.ui = true;
            }
            self.trigger(lane, false);
        }
        taken[pos].1 = part(0);
        let (n, k) = (list.len(), lanes.len());
        let copies = if k == 2 { "a copy".to_string() } else { format!("{} copies", k - 1) };
        self.toast("info", &format!("{} listed {n} jobs: {} and {copies} of it take them, at the same time.", whole.name, node.name));
        taken
    }

    /// The cards that take a list of `want` shares: `id` and its copies, made as needed and set up as
    /// it is now (its prompt, agent, model, checks).
    fn lanes(&mut self, id: &str, link: &Edge, want: usize) -> Vec<String> {
        let Some(orig) = self.node(id).cloned() else { return Vec::new() };
        let mut lanes = vec![id.to_string()];
        for part in 1..want as u32 {
            let had = self.flow.nodes.iter().find(|n| n.each_of == id && n.part == part).map(|n| n.id.clone());
            let cid = match had {
                Some(c) => {
                    self.sync_copy(&orig, &c);
                    c
                }
                None => self.make_copy(&orig, part),
            };
            self.push_edge(&link.from, &cid, link.on, true);
            lanes.push(cid);
        }
        self.flow_changed();
        lanes
    }

    /// A copy of `orig` for share `part`, under the cards that take the list, linked on as it is.
    fn make_copy(&mut self, orig: &Node, part: u32) -> String {
        let bottom = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.id == orig.id || n.each_of == orig.id)
            .map(|n| n.y + super::marks::card_h(n))
            .fold(orig.y, f64::max);
        let mut c = orig.clone();
        c.id = self.next_id("n", self.taken_ids(false).into_iter());
        c.name = self.unique_name(&orig.name, None);
        c.auto_name = false;
        c.each_of = orig.id.clone();
        c.part = part;
        c.draft = false;
        c.keep = false;
        c.takeover.clear();
        c.twin.clear();
        c.fixes.clear();
        (c.x, c.y) = self.free_spot(orig.x, bottom + 60.0);
        let cid = c.id.clone();
        self.rt.insert(cid.clone(), Rt::new(self.cfg.cols, self.cfg.rows));
        self.flow.nodes.push(c);
        self.mirror(&orig.id, &cid, (&orig.id, &cid), &mut HashSet::new());
        cid
    }

    /// A copy made for an earlier list works as the card it copies does now.
    fn sync_copy(&mut self, orig: &Node, cid: &str) {
        let Some(c) = self.flow.nodes.iter_mut().find(|n| n.id == cid) else { return };
        c.agent = orig.agent.clone();
        c.cmd = orig.cmd.clone();
        c.cwd = orig.cwd.clone();
        c.prompt = orig.prompt.clone();
        c.addenda = orig.addenda.clone();
        c.finish = orig.finish;
        c.quiet_s = orig.quiet_s;
        c.worktree = orig.worktree;
        c.model = orig.model.clone();
        c.effort = orig.effort.clone();
        c.yolo = orig.yolo;
        c.checks = orig.checks.clone();
        c.draft = false;
    }

    /// What `from` links on to, linked on from its copy `to` as well. A commit (one agent's work), a
    /// check on the link and a prompt queued on the agent belong to one card: the copy gets its own.
    /// `agent`: the card that is copied and its copy (whose session a copied queued prompt is asked in).
    fn mirror(&mut self, from: &str, to: &str, agent: (&str, &str), seen: &mut HashSet<String>) {
        let outs: Vec<Edge> = self.flow.edges.iter().filter(|e| e.from == from).cloned().collect();
        let dy = self.node(to).map_or(0.0, |n| n.y) - self.node(from).map_or(0.0, |n| n.y);
        for o in outs {
            let Some(x) = self.node(&o.to).cloned() else { continue };
            let own = match x.kind() {
                Kind::Git => x.git_op == GitOp::Commit,
                Kind::Step => x.host == agent.0,
                k => k.is_gate(),
            };
            let next = if own && seen.insert(x.id.clone()) {
                let mut c = x.clone();
                c.id = self.next_id("n", self.taken_ids(false).into_iter());
                c.name = self.unique_name(&x.name, None);
                c.fixes.clear();
                c.keep = false;
                c.each_of = x.id.clone();
                c.part = self.node(agent.1).map_or(0, |n| n.part);
                if c.kind() == Kind::Step {
                    c.host = agent.1.to_string();
                }
                (c.x, c.y) = self.free_spot(x.x, x.y + dy);
                let cid = c.id.clone();
                self.rt.insert(cid.clone(), Rt::new(self.cfg.cols, self.cfg.rows));
                self.flow.nodes.push(c);
                self.mirror(&x.id, &cid, agent, seen);
                cid
            } else {
                x.id
            };
            self.push_edge(to, &next, o.on, false);
        }
    }

    /// A link weft draws itself, with nothing going down it yet.
    fn push_edge(&mut self, from: &str, to: &str, on: On, each: bool) {
        if let Some(e) = self.flow.edges.iter_mut().find(|e| e.from == from && e.to == to) {
            (e.on, e.each) = (on, each);
            return;
        }
        let id = self.next_id("e", self.taken_ids(true).into_iter());
        self.flow.edges.push(Edge { id, from: from.into(), to: to.into(), on, each });
    }

    /// What a link hands its card when that takes it up again (a retry, a new session): along a
    /// "for each" link the card's own share of the list, as it was given it.
    pub(super) fn input_along(&self, e: &Edge) -> Input {
        let (name, ok, text) = self.input_from(&e.from);
        let whole = Input { from: e.from.clone(), name, ok, text, kind: InputKind::Result };
        if !e.each {
            return whole;
        }
        let list = items(&whole.text);
        let lane = self.node(&e.to).map_or(0, |n| n.part as usize);
        match shares(list.len(), self.each_max()).get(lane) {
            Some(s) => {
                Input { kind: InputKind::Part, text: words::part(&list, s.clone(), &self.piped(&e.from, whole.text.trim())), ..whole }
            }
            None => whole,
        }
    }

    /// The card a "for each" link leads to goes: its first copy takes its place, the others are
    /// copies of that one now.
    pub(super) fn copies_lose(&mut self, gone: &str) {
        let first = self.flow.nodes.iter().filter(|n| n.each_of == gone).min_by_key(|n| n.part).map(|n| n.id.clone());
        let Some(first) = first else { return };
        for n in self.flow.nodes.iter_mut() {
            if n.id == first {
                n.each_of.clear();
                n.part = 0;
            } else if n.each_of == gone {
                n.each_of = first.clone();
            }
        }
    }

    /// Why a link from `from` to `to` cannot be "for each" (`edge`: that link, when it is there).
    pub(super) fn each_refused(&self, from: &str, to: &str, edge: Option<&str>) -> Option<&'static str> {
        let (Some(f), Some(t)) = (self.node(from), self.node(to)) else { return Some("invalid link") };
        if f.kind() == Kind::Note {
            Some("a note is not an answer that lists jobs: put an agent before the link that lists them")
        } else if !takes_jobs(t) {
            Some("for each makes a copy of the agent after the link for every job: this link does not lead to an agent")
        } else if !t.each_of.is_empty() {
            Some("this card is a copy that takes a share of another link's list")
        } else if self.flow.edges.iter().any(|x| x.to == to && x.each && Some(x.id.as_str()) != edge && x.from != from) {
            Some("another link into this card already shares out a list")
        } else {
            None
        }
    }

    /// Switch a link to "for each" or back. Back: its copies stay, but nothing comes down to them now.
    pub(super) fn cmd_edge_each(&mut self, id: String, each: bool) -> Value {
        let Some(e) = self.flow.edges.iter().find(|e| e.id == id).cloned() else {
            return json!({"ok": false, "error": "no such link"});
        };
        if let Some(why) = self.each_refused(&e.from, &e.to, Some(&e.id)).filter(|_| each) {
            return json!({"ok": false, "error": why});
        }
        if let Some(x) = self.flow.edges.iter_mut().find(|x| x.id == id) {
            x.each = each;
        }
        if !each {
            let copies: Vec<String> = self.flow.nodes.iter().filter(|n| n.each_of == e.to).map(|n| n.id.clone()).collect();
            let gone: Vec<String> =
                self.flow.edges.iter().filter(|x| x.from == e.from && copies.contains(&x.to)).map(|x| x.id.clone()).collect();
            for g in gone {
                self.cmd_edge_del(g);
            }
        }
        self.flow_changed();
        json!({"ok": true})
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    const PLAN: &str = "I looked at the module. Three things are broken.\n\n\
Jobs:\n1. Fix alpha in a.py: it drops the last line.\n2. Fix beta in b.py\n   (the slow loop in `scan`)\n3. Fix gamma in c.py.\n\nSTATUS: done";

    fn copies(e: &Engine, of: &str) -> Vec<Node> {
        let mut c: Vec<Node> = e.flow.nodes.iter().filter(|n| n.each_of == of).cloned().collect();
        c.sort_by_key(|n| n.part);
        c
    }

    /// `id` took up its queued prompt and answered `out`
    fn ran(e: &mut Engine, id: &str, out: &str) {
        let r = e.rt.get_mut(id).unwrap();
        r.queue.clear();
        r.runs += 1;
        r.cause = Cause::Flow;
        r.armed = true;
        r.finished = false;
        e.settle(id, true, out.into());
    }

    fn each_link(e: &mut Engine, id: &str, from: &str, to: &str) {
        link(e, id, from, to, On::Done);
        e.flow.edges.last_mut().unwrap().each = true;
    }

    #[test]
    fn the_jobs_are_the_last_numbered_list() {
        assert_eq!(
            items(PLAN),
            vec!["Fix alpha in a.py: it drops the last line.", "Fix beta in b.py\n(the slow loop in `scan`)", "Fix gamma in c.py."]
        );
        // an earlier list is not it, nor a list inside an item, nor one in a code block
        let t = "Steps I took:\n1. read\n2. ran\n\nNow:\n1. one\n   - sub a\n   - sub b\n2) two\n```\n3. not this\n```\n";
        assert_eq!(items(t), vec!["one\n- sub a\n- sub b", "two\n```\n3. not this\n```"]);
        // bullets when there is no numbered list, to-do boxes left out
        assert_eq!(items("To do:\n- [ ] a\n- [x] b\n* c"), vec!["a", "b", "c"]);
        // one job, or none, is nothing to share out
        assert!(items("1. only this").is_empty());
        assert!(items("no list at all").is_empty());
        assert!(items("1.5 is a number, 2.0 too").is_empty());
    }

    #[test]
    fn a_long_list_is_shared_out_in_runs() {
        let lens = |n, m| shares(n, m).iter().map(|r: &Range<usize>| r.len()).collect::<Vec<_>>();
        assert_eq!(lens(3, 6), vec![1, 1, 1]);
        assert_eq!(lens(7, 6), vec![2, 1, 1, 1, 1, 1]);
        assert_eq!(lens(20, 6), vec![4, 4, 3, 3, 3, 3]);
        assert_eq!(shares(20, 6)[1], 4..8);
        assert!(shares(0, 6).is_empty());
    }

    #[test]
    fn each_job_goes_to_its_own_copy_and_what_comes_after_waits_for_all() {
        let mut e = engine();
        add(&mut e, "a", "plan", "find the bugs");
        add(&mut e, "t", "fixer", "Fix this: {{inputs}}");
        add(&mut e, "r", "report", "Sum up {{inputs}}");
        each_link(&mut e, "e1", "a", "t");
        link(&mut e, "e2", "t", "r", On::Done);
        // the planner is asked to end with the jobs
        let (_, full) = e.render_parts(&e.node("a").unwrap().clone(), &[]);
        assert!(full.contains("numbered") && full.contains("Up to 6 agents"), "{full}");

        e.settle("a", true, PLAN.into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 2, "two copies for three jobs");
        assert_eq!((c[0].part, c[1].part), (1, 2));
        assert_eq!((c[0].name.as_str(), c[0].prompt.as_str()), ("fixer 2", "Fix this: {{inputs}}"));
        let job = |id: &str| e.rt[id].queue.front().map(|q| q.text.clone()).unwrap_or_default();
        assert!(job("t").contains("item 1 of 3") && job("t").contains("1. Fix alpha"), "{}", job("t"));
        assert!(job(&c[0].id).contains("item 2 of 3") && job(&c[0].id).contains("2. Fix beta"), "{}", job(&c[0].id));
        assert!(job(&c[1].id).contains("item 3 of 3"));
        assert!(job("t").contains("The whole answer the list is from"), "with the plan for what they share");
        // the copies are linked from the planner (for each) and on to the report
        for x in &c {
            assert!(e.flow.edges.iter().any(|l| l.from == "a" && l.to == x.id && l.each));
            assert!(e.flow.edges.iter().any(|l| l.from == x.id && l.to == "r"));
        }
        // the report waits for every one of them
        ran(&mut e, "t", "fixed alpha");
        ran(&mut e, &c[0].id, "fixed beta");
        assert!(e.rt["r"].queue.is_empty(), "one job is still open");
        ran(&mut e, &c[1].id, "fixed gamma");
        let q = &e.rt["r"].queue[0].text;
        assert!(q.contains("fixed alpha") && q.contains("fixed beta") && q.contains("fixed gamma"), "{q}");

        // a second list: the copies there are take it again, one more is made for a longer one
        let more = "1. a\n2. b\n3. c\n4. d";
        e.settle("a", true, more.into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 3);
        assert!(e.rt[&c[2].id].queue.front().is_some_and(|q| q.text.contains("item 4 of 4")));
    }

    #[test]
    fn a_commit_after_the_card_is_one_per_copy_and_they_meet_at_the_push() {
        let mut e = engine();
        add(&mut e, "a", "plan", "find the bugs");
        add(&mut e, "t", "fixer", "Fix: {{inputs}}");
        add_kind(&mut e, "g", "approve", "gate", "");
        add_kind(&mut e, "c", "commit", "git", "");
        add_kind(&mut e, "p", "push", "git", "");
        e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().git_op = GitOp::Commit;
        e.flow.nodes.iter_mut().find(|n| n.id == "p").unwrap().git_op = GitOp::Push;
        each_link(&mut e, "e1", "a", "t");
        link(&mut e, "e2", "t", "g", On::Ok);
        link(&mut e, "e3", "g", "c", On::Done);
        link(&mut e, "e4", "c", "p", On::Done);
        e.settle("a", true, "1. one\n2. two".into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 1);
        let after = |id: &str| e.flow.edges.iter().filter(|l| l.from == id).map(|l| (l.to.clone(), l.on)).collect::<Vec<_>>();
        let [(gate, on)] = &after(&c[0].id)[..] else { panic!("{:?}", after(&c[0].id)) };
        assert!(gate != "g" && *on == On::Ok, "its own check, on the same condition");
        let [(commit, _)] = &after(gate)[..] else { panic!() };
        assert!(commit != "c" && e.node(commit).unwrap().git_op == GitOp::Commit, "its own commit");
        assert_eq!(after(commit), vec![("p".to_string(), On::Done)], "into the same push");
        assert_eq!(e.flow.edges.iter().filter(|l| l.to == "p").count(), 2);
    }

    #[test]
    fn no_list_means_the_whole_answer_and_a_long_one_is_shared_out() {
        let mut e = engine();
        add(&mut e, "a", "plan", "x");
        add(&mut e, "t", "fixer", "Do {{inputs}}");
        each_link(&mut e, "e1", "a", "t");
        e.settle("a", true, "Nothing to split here, all fine.".into());
        assert!(copies(&e, "t").is_empty());
        assert!(e.rt["t"].queue[0].text.contains("Nothing to split here"));

        let mut e = engine();
        add(&mut e, "a", "plan", "x");
        add(&mut e, "t", "fixer", "Do {{inputs}}");
        each_link(&mut e, "e1", "a", "t");
        e.cfg.each_max = 2;
        e.settle("a", true, "1. a\n2. b\n3. c\n4. d\n5. e".into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 1, "two cards at most");
        assert!(e.rt["t"].queue[0].text.contains("items 1 to 3 of 5"));
        assert!(e.rt[&c[0].id].queue[0].text.contains("items 4 to 5 of 5"));
    }

    #[test]
    fn a_draft_shares_out_when_it_is_armed_with_what_it_says_then() {
        let mut e = engine();
        add(&mut e, "a", "plan", "x");
        add(&mut e, "t", "fixer", "");
        e.flow.nodes.iter_mut().find(|n| n.id == "t").unwrap().draft = true;
        each_link(&mut e, "e1", "a", "t");
        e.settle("a", true, "1. a\n2. b".into());
        assert!(copies(&e, "t").is_empty(), "nothing is copied before it runs");
        e.flow.nodes.iter_mut().find(|n| n.id == "t").unwrap().prompt = "Fix: {{inputs}}".into();
        e.cmd_arm("t".into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].prompt, "Fix: {{inputs}}", "the copy has the prompt typed in the meantime");
        assert!(!c[0].draft);
        assert!(e.rt[&c[0].id].queue[0].text.contains("item 2 of 2"));
    }

    #[test]
    fn a_link_drawn_as_for_each_shares_out_what_its_card_has_already() {
        // (undo, a network, a link moved by its end: it comes back "for each" before the result goes down it)
        let mut e = engine();
        add(&mut e, "a", "plan", "x");
        add(&mut e, "t", "fixer", "Fix: {{inputs}}");
        add_kind(&mut e, "g", "commit", "git", "");
        e.settle("a", true, "1. a\n2. b".into());
        assert_eq!(e.cmd_edge_add("a".into(), "t".into(), On::Done, true)["ok"], true);
        assert!(e.flow.edges.iter().any(|l| l.from == "a" && l.to == "t" && l.each));
        assert_eq!(copies(&e, "t").len(), 1);
        assert!(e.rt["t"].queue[0].text.contains("item 1 of 2"));
        // one that cannot share out a list is drawn plain
        assert_eq!(e.cmd_edge_add("t".into(), "g".into(), On::Done, true)["ok"], true);
        assert!(e.flow.edges.iter().any(|l| l.from == "t" && l.to == "g" && !l.each));
    }

    #[test]
    fn what_a_link_can_share_out_and_what_happens_to_the_copies() {
        let mut e = engine();
        add(&mut e, "a", "plan", "x");
        add(&mut e, "t", "fixer", "Fix: {{inputs}}");
        add_kind(&mut e, "n", "style", "note", "Keep the style guide.");
        add_kind(&mut e, "g", "commit", "git", "");
        link(&mut e, "e1", "a", "t", On::Done);
        link(&mut e, "e2", "n", "t", On::Done);
        add(&mut e, "z", "other", "x");
        link(&mut e, "e3", "z", "g", On::Done);
        assert_eq!(e.cmd_edge_each("e2".into(), true)["ok"], false, "a note lists no jobs");
        assert_eq!(e.cmd_edge_each("e3".into(), true)["ok"], false, "a git step is no agent to copy");
        assert_eq!(e.cmd_edge_each("e1".into(), true)["ok"], true);
        e.settle("a", true, "1. a\n2. b\n3. c".into());
        let c = copies(&e, "t");
        assert_eq!(c.len(), 2);
        assert!(e.rt[&c[0].id].queue[0].text.contains("Keep the style guide."), "what else the card was given goes to its copies too");
        // the card goes: its first copy takes its place
        e.cmd_node_del("t".into());
        let first = c[0].id.clone();
        assert!(e.node(&first).unwrap().each_of.is_empty());
        assert_eq!(e.node(&c[1].id).unwrap().each_of, first);
        // back to a plain link: the copies stay, the links to them go
        let link_id = e.flow.edges.iter().find(|l| l.from == "a" && l.to == first).unwrap().id.clone();
        e.cmd_edge_each(link_id, false);
        assert!(e.node(&c[1].id).is_some());
        assert!(!e.flow.edges.iter().any(|l| l.from == "a" && l.to == c[1].id));
    }
}
