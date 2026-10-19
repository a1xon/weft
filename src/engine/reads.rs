//! What a report leaves to the person, read by a model. The report an agent gives on the person's
//! task (`weft done`, or the answer it goes quiet on) is read by its own CLI (`asks_cmd`: claude's
//! sonnet at medium reasoning, ...) for the questions and decisions it leaves to them, each with
//! answers to pick (`asks::read`). The card stays at work until the reading is in: a report that
//! leaves nothing goes on, one that asks is held for the person (`Engine::hold_or_go`). An ISSUE is
//! read the same way for its answers to pick. weft's own rules (`asks::asks`) read it when the CLI
//! has no such command, or the reading fails.
//!
//! Why a model: the rules took a recap of the person's own answers ("Your decisions: 88 duplicate
//! tasks dismissed ...") for decisions left open and held a finished crawl (rubbermetrics n142), and
//! missed questions that end in a recommendation. On the reports saved on every board the smallest
//! models made the same kind of mistakes; sonnet at medium told them apart and wrote good answers.

use super::*;

/// A reading that has not answered by then is given up: the rules read the report instead.
const TIMEOUT: Duration = Duration::from_secs(150);

impl Engine {
    /// The command that reads a card's reports: its own agent's.
    fn asks_cmd(&self, id: &str) -> Option<String> {
        let agent = &self.node(id)?.agent;
        self.cfg.agents.get(agent).map(|c| c.asks_cmd.trim().to_string()).filter(|c| self.cfg.asks && !c.is_empty())
    }

    /// Have a model read `text`, the report (or the ISSUE, `issue`) of the step the card is at; the
    /// step keeps the reading's ticket. False when no model reads this card's reports.
    pub(super) fn read_asks(&mut self, id: &str, text: &str, issue: bool) -> bool {
        let Some(cmd) = self.asks_cmd(id) else { return false };
        let Some(rt) = self.rt.get_mut(id) else { return false };
        let Some(c) = rt.cur.as_mut() else { return false };
        // (what comes back for an older reading is dropped)
        let ticket = now_ms().max(c.reading + 1);
        c.reading = ticket;
        let req = words::asks_request(text, &rt.asked, issue);
        let (tx, id, text) = (self.tx.clone(), id.to_string(), text.to_string());
        tokio::spawn(async move {
            let asks = run_oneshot(&cmd, "WEFT_ASKS_REQ", &req, TIMEOUT).await.and_then(|out| asks::read(&out, &text));
            let _ = tx.send(Ev::Asks { id, ticket, asks });
        });
        true
    }

    /// The agent raised an ISSUE: its questions get answers to pick (the rules' until the reading is in).
    pub(super) fn read_issue(&mut self, id: &str) -> bool {
        let Some(note) = self.rt.get(id).and_then(|r| r.issue.clone()) else { return false };
        self.read_asks(id, &note, true)
    }

    /// A reading is in (`None`: it failed). A report goes on, or is held for what it asks; an ISSUE
    /// shows the questions the model told apart in it.
    pub(super) fn on_asks(&mut self, id: &str, ticket: u64, read: Option<Vec<asks::Ask>>) {
        let Some(rt) = self.rt.get_mut(id) else { return };
        let Some(c) = rt.cur.as_mut().filter(|c| c.reading == ticket && ticket != 0) else { return };
        c.reading = 0;
        rt.ui = true;
        self.state_dirty = true;
        let Some(report) = c.report.clone() else {
            c.asks = read.filter(|a| asks::waits(a));
            return;
        };
        let found = match read {
            Some(a) => {
                let open: Vec<asks::Ask> = a.into_iter().filter(|a| !c.answered.contains(&asked_key(&a.text))).collect();
                c.asks = Some(open.clone());
                open
            }
            None => c.asks_in(&report),
        };
        self.hold_or_go(id, Some(report), found, How::Signal);
    }

    /// After a restart: a reading that was on its way is asked for again, and a report the rules held
    /// is read by the model (the daemon that held it may have had none to ask).
    pub(super) fn reread(&mut self, id: &str) {
        let Some(rt) = self.rt.get(id) else { return };
        let Some(c) = rt.cur.as_ref() else { return };
        let (reading, held) = (c.reading, c.report.is_some() && c.asks.is_none() && rt.issue.is_some());
        let started = match (&c.report, &rt.issue) {
            (Some(r), _) if reading != 0 || held => {
                let r = r.clone();
                self.read_asks(id, &r, false)
            }
            (None, Some(_)) if reading != 0 => self.read_issue(id),
            _ => return,
        };
        if !started && reading != 0 {
            // nothing to ask now: the rules decide
            self.on_asks(id, reading, None);
        }
    }
}
