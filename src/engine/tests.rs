//! Unit tests of the engine's logic: no tmux, no network, one actor in a scratch folder.

use super::git::*;
use super::info::*;
use super::*;

#[test]
fn expand_substitutes_once_and_keeps_unknown() {
    let f = |k: &str| match k {
        "a" => Some("{{b}}".to_string()),
        "b" => Some("B".to_string()),
        _ => None,
    };
    // substituted text is never re-scanned
    assert_eq!(expand("x {{a}} y {{b}} z {{nope}}", &f), "x {{b}} y B z {{nope}}");
    assert_eq!(expand("dangling {{a", &f), "dangling {{a");
    assert_eq!(expand("no vars", &f), "no vars");
}

#[test]
fn tail_bytes_respects_char_boundaries() {
    // "ö" is two bytes: cutting inside it must round up, never panic
    assert_eq!(tail_bytes("héllo wörld", 5), "örld");
    assert_eq!(tail_bytes("héllo wörld", 4), "rld");
    assert_eq!(tail_bytes("short", 50), "short");
}

#[test]
fn titles_are_short_and_do_not_end_on_stop_words() {
    assert_eq!(title_from("Fix the slugify function in textkit/slug.py so that tests pass", "x"), "Fix the slugify function");
    assert_eq!(title_from("Review the work from the previous step: look for bugs", "x"), "Review the work");
    assert_eq!(title_from("Explain what textkit/slug.py does in one sentence.", "x"), "Explain what textkit/slug.py");
    assert_eq!(title_cmd("python3 -m unittest discover -s tests -q", "x"), "python3 -m unittest discover");
    assert_eq!(title_from("", "claude"), "claude");
    assert_eq!(title_from("   \n  ", "Agent"), "Agent");
}

fn engine() -> Engine {
    // every engine gets its own scratch folder: tests run in parallel and write files
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let scratch =
        PathBuf::from(format!("/tmp/weft-unit-tests-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let (tx, _rx) = mpsc::unbounded_channel();
    let boot = Boot {
        config_file: None,
        dir: scratch.clone(),
        name: "t".into(),
        session: "s".into(),
        sock: "x".into(),
        exe: "/bin/weft".into(),
        url: "http://127.0.0.1:1".into(),
        token: "t".into(),
    };
    // no unit test asks a real CLI to name a card
    let mut e = Engine::new(boot, Config { titles: false, ..Config::default() }, tx, None);
    // unit tests write only into a scratch folder
    e.flow_path = scratch.join("flow.json");
    e
}

fn node(id: &str, name: &str, prompt: &str) -> Node {
    serde_json::from_value(json!({"id": id, "name": name, "prompt": prompt, "addenda": []})).unwrap()
}

fn add(e: &mut Engine, id: &str, name: &str, prompt: &str) {
    e.flow.nodes.push(node(id, name, prompt));
    e.rt.insert(id.into(), Rt::new(80, 24));
}

fn link(e: &mut Engine, id: &str, from: &str, to: &str, on: On) {
    e.flow.edges.push(Edge { id: id.into(), from: from.into(), to: to.into(), on });
}

#[test]
fn render_places_inputs_and_addenda() {
    let mut e = engine();
    let ins = vec![
        Input { from: "n1".into(), name: "plan".into(), ok: true, text: "do {{weft}} things".into(), kind: InputKind::Result },
        Input { from: "n2".into(), name: "tests".into(), ok: true, text: "all green".into(), kind: InputKind::Result },
    ];
    // no placeholder: the context block is appended
    let n = node("n3", "impl", "Implement it.");
    let out = e.render(&n, &ins);
    assert!(out.starts_with("Implement it.\n\n## Results of the previous steps"));
    assert!(out.contains("### Output of \"plan\"\ndo {{weft}} things"), "agent output must not be re-expanded");
    // explicit placeholders
    let n = node("n3", "impl", "A:\n{{from:tests}}\nB:\n{{inputs}}");
    let out = e.render(&n, &ins);
    assert!(out.contains("A:\nall green\nB:\n### Output of \"plan\""));
    assert!(!out.contains("Results of the previous steps"));
    // addenda are referenced and variables expand; `shown` excludes them
    let mut n = node("n3", "impl", "Go.");
    n.addenda = vec!["report-done".into()];
    let (shown, full) = e.render_parts(&n, &[]);
    assert_eq!(shown, "Go.");
    assert!(full.contains("/bin/weft done <<'EOF'"));
    // a mention of an agent that has not arrived falls back to its latest result
    add(&mut e, "n9", "researcher", "x");
    e.rt.get_mut("n9").unwrap().output = Some("42 findings".into());
    let n = node("n3", "impl", "Use {{from:researcher}} and {{from:ghost}}.");
    assert_eq!(e.render(&n, &[]), "Use 42 findings and (no output from ghost yet).");
}

#[test]
fn promise_all_waits_for_every_input_and_blocks_on_a_failure() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "go");
    add(&mut e, "n3", "c", "go");
    link(&mut e, "e1", "n1", "n3", On::Done);
    link(&mut e, "e2", "n2", "n3", On::Ok);
    // a settles: c keeps waiting
    e.settle("n1", true, "A done".into());
    assert_eq!(e.rt["n3"].inputs.len(), 1);
    assert_eq!(e.rt["n3"].queue.len(), 0);
    let none = HashMap::new();
    assert_eq!(e.eff_status("n3", &none), Status::Waiting);
    // b fails: its link is "only if it succeeds", so the junction can never be satisfied
    e.settle("n2", false, "boom".into());
    assert_eq!(e.rt["n3"].queue.len(), 0);
    let blocked = e.blocked_map();
    assert_eq!(blocked.get("n3").map(String::as_str), Some("b"));
    assert_eq!(e.eff_status("n3", &blocked), Status::Blocked);
    // b succeeds on a retry: the junction opens and c receives both results as one prompt
    e.settle("n2", true, "B done".into());
    assert!(e.rt["n3"].inputs.is_empty());
    assert_eq!(e.rt["n3"].queue.len(), 1);
    let q = &e.rt["n3"].queue[0];
    assert!(q.text.contains("A done") && q.text.contains("B done"), "{}", q.text);
    assert_eq!(q.src, Src::Flow);
    assert!(e.blocked_map().is_empty());
}

#[test]
fn promise_any_fires_once_and_the_loop_guard_stops_runaways() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "go");
    add(&mut e, "n3", "c", "go");
    e.flow.nodes[2].join = Join::Any;
    e.cfg.max_runs = 3;
    link(&mut e, "e1", "n1", "n3", On::Done);
    link(&mut e, "e2", "n2", "n3", On::Done);
    e.settle("n1", true, "x".into());
    assert_eq!(e.rt["n3"].queue.len(), 1, "any fires on the first input");
    // c "ran" three times already: further automatic triggers are refused
    e.rt.get_mut("n3").unwrap().runs = 3;
    e.settle("n2", true, "y".into());
    assert_eq!(e.rt["n3"].queue.len(), 1, "loop guard caps automatic re-triggers");
}

#[test]
fn connecting_to_a_settled_agent_delivers_immediately() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "review {{inputs}}");
    e.settle("n1", true, "the plan".into());
    assert!(e.rt["n2"].queue.is_empty());
    e.add_edge("n1", "n2", On::Done).unwrap();
    assert_eq!(e.rt["n2"].queue.len(), 1);
    assert!(e.rt["n2"].queue[0].text.contains("the plan"));
    // a failed result does not satisfy an "only if it succeeds" link
    add(&mut e, "n3", "c", "x");
    e.settle("n2", false, "nope".into());
    e.add_edge("n2", "n3", On::Ok).unwrap();
    assert!(e.rt["n3"].queue.is_empty());
    assert!(e.rt["n3"].inputs.is_empty());
}

#[test]
fn a_result_from_chatting_waits_for_a_hand_over() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "next {{inputs}}");
    link(&mut e, "e1", "n1", "n2", On::Done);
    // first run is caused by the flow: it propagates
    e.rt.get_mut("n1").unwrap().cause = Cause::Flow;
    e.settle("n1", true, "first".into());
    assert_eq!(e.rt["n2"].queue.len(), 1);
    e.rt.get_mut("n2").unwrap().queue.clear();
    // the user chats with the settled agent: that run does not cascade
    {
        let r = e.rt.get_mut("n1").unwrap();
        r.finished = false;
        r.cause = Cause::User;
    }
    e.settle("n1", true, "second".into());
    assert!(e.rt["n2"].queue.is_empty());
    assert!(e.rt["n1"].fresh, "the result is flagged as not handed over");
}

#[test]
fn script_continues_until_the_queue_drains() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "after {{inputs}}");
    link(&mut e, "e1", "n1", "n2", On::Done);
    {
        let r = e.rt.get_mut("n1").unwrap();
        r.armed = true;
        r.cur = Some(Step::new("one".into(), vec![]));
        r.queue.push_back(QItem { id: 1, text: "two".into(), shown: "two".into(), src: Src::You });
    }
    e.step_done("n1", true, Some("result one".into()), How::Signal);
    // a prompt is still queued: no settle, no propagation yet
    assert!(!e.rt["n1"].finished);
    assert!(e.rt["n2"].queue.is_empty() && e.rt["n2"].inputs.is_empty());
    e.rt.get_mut("n1").unwrap().queue.clear();
    e.rt.get_mut("n1").unwrap().cur = Some(Step::new("two".into(), vec![]));
    e.step_done("n1", true, Some("result two".into()), How::Signal);
    assert!(e.rt["n1"].finished);
    assert_eq!(e.rt["n1"].steps.len(), 2);
    assert!(e.rt["n2"].queue[0].text.contains("result two"));
}

fn add_kind(e: &mut Engine, id: &str, name: &str, agent: &str, prompt: &str) {
    let n: Node = serde_json::from_value(json!({"id": id, "name": name, "agent": agent, "prompt": prompt, "addenda": []})).unwrap();
    e.flow.nodes.push(n);
    e.rt.insert(id.into(), Rt::new(80, 24));
    if agent == "note" {
        e.refresh_note(id);
    }
}

#[test]
fn a_draft_collects_inputs_and_runs_when_armed() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "review {{inputs}}");
    e.flow.nodes[1].draft = true;
    link(&mut e, "e1", "n1", "n2", On::Done);
    e.settle("n1", true, "the plan".into());
    assert!(e.rt["n2"].queue.is_empty(), "a draft must not run");
    assert_eq!(e.rt["n2"].inputs.len(), 1, "but it keeps what arrived");
    assert_eq!(e.eff_status("n2", &HashMap::new()), Status::Draft);
    let r = e.cmd(None, Cmd::Arm { id: "n2".into() });
    assert_eq!(r["ok"], true);
    assert_eq!(e.rt["n2"].queue.len(), 1);
    assert!(e.rt["n2"].queue[0].text.contains("the plan"));
}

#[test]
fn notes_are_constants_that_ride_along_but_never_start_anything() {
    let mut e = engine();
    add_kind(&mut e, "n1", "brief", "note", "Use tabs, never spaces.");
    add(&mut e, "n2", "a", "go");
    add(&mut e, "n3", "c", "write {{inputs}}");
    // linking a note into an idle agent does not run it
    e.add_edge("n1", "n3", On::Done).unwrap();
    assert!(e.rt["n3"].queue.is_empty());
    // the real input arrives: the note's text is part of the prompt
    link(&mut e, "e9", "n2", "n3", On::Done);
    e.settle("n2", true, "plan".into());
    assert_eq!(e.rt["n3"].queue.len(), 1);
    let t = &e.rt["n3"].queue[0].text;
    assert!(t.contains("Use tabs, never spaces.") && t.contains("plan"), "{t}");
    // editing the note updates what an agent will receive next time
    e.flow.nodes[0].prompt = "Use spaces.".into();
    e.refresh_note("n1");
    assert_eq!(e.const_inputs("n3")[0].1.text, "Use spaces.");
    // a note takes no input
    assert!(e.add_edge("n2", "n1", On::Done).is_err());
}

#[test]
fn an_approval_gate_holds_the_flow_until_a_human_decides() {
    let mut e = engine();
    add(&mut e, "n1", "plan", "go");
    add_kind(&mut e, "n2", "approve", "gate", "");
    add(&mut e, "n3", "implement", "do {{inputs}}");
    add(&mut e, "n4", "rework", "again {{inputs}}");
    link(&mut e, "e1", "n1", "n2", On::Done);
    link(&mut e, "e2", "n2", "n3", On::Ok);
    link(&mut e, "e3", "n2", "n4", On::Fail);
    e.settle("n1", true, "step 1, step 2".into());
    assert!(e.rt["n2"].gate.held.as_deref().unwrap_or("").contains("step 1, step 2"));
    assert_eq!(e.rt["n2"].status, Status::Attention);
    assert!(e.rt["n3"].queue.is_empty() && e.rt["n4"].queue.is_empty(), "nothing moves while it waits");
    assert_eq!(e.cmd(None, Cmd::Approve { id: "n2".into(), ok: true })["ok"], true);
    assert_eq!(e.rt["n3"].queue.len(), 1);
    assert!(e.rt["n3"].queue[0].text.contains("step 1, step 2"), "approval passes the held results on");
    assert!(e.rt["n4"].queue.is_empty());
    // a second round, rejected this time
    e.rt.get_mut("n2").unwrap().finished = false;
    e.settle("n1", true, "plan v2".into());
    e.cmd(None, Cmd::Approve { id: "n2".into(), ok: false });
    assert_eq!(e.rt["n4"].queue.len(), 1, "a rejection fires the fail link");
}

#[test]
fn commit_subject_skips_rule_lines_and_titles_follow_the_command() {
    let n: Node = serde_json::from_value(json!({"id": "n1", "name": "tests"})).unwrap();
    let out = "----------------------------------------------------------------------\nRan 9 tests in 0.000s\n\nOK";
    assert_eq!(commit_subject(&n, "tests", out), "tests: Ran 9 tests in 0.000s");
    // a shell node is retitled when its command is set after creation
    let mut e = engine();
    let r = e.cmd(None, Cmd::NodeAdd { node: json!({"agent": "sh", "draft": true}), from: None, run: false });
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(e.node(&id).unwrap().name, "sh");
    e.cmd(None, Cmd::NodeSet { id: id.clone(), patch: json!({"cmd": "python3 -m unittest discover -q"}) });
    assert_eq!(e.node(&id).unwrap().name, "python3 -m unittest discover");
}

#[test]
fn commit_subjects_are_short_and_use_the_template() {
    let n: Node = serde_json::from_value(json!({"id": "n1", "name": "ship"})).unwrap();
    assert_eq!(commit_subject(&n, "fix slug", "\u{25CF} Updated textkit/slug.py\nmore"), "fix slug: Updated textkit/slug.py");
    let mut n2 = n.clone();
    n2.message = "feat({{agent}}): {{result}}".into();
    assert_eq!(commit_subject(&n2, "fix wrap", "Fixed off-by-one"), "feat(fix wrap): Fixed off-by-one");
    let long = "x".repeat(200);
    assert!(commit_subject(&n, "a", &long).chars().count() <= 73);
}

fn alive_agent(e: &mut Engine, id: &str, name: &str) {
    add(e, id, name, "build it");
    e.rt.get_mut(id).unwrap().alive = true;
}

#[test]
fn a_screenshot_gate_asks_the_agent_shows_the_image_and_is_a_barrier() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    add_kind(&mut e, "g", "screenshot", "shot", "");
    add(&mut e, "b", "deploy", "ship {{inputs}}");
    link(&mut e, "e1", "a", "g", On::Done);
    link(&mut e, "e2", "g", "b", On::Ok);
    e.flow.nodes[1].subject = "the checkout page".into();

    e.settle("a", true, "built the checkout".into());
    // the gate has asked the upstream agent, in its own session, ahead of anything else
    let req = e.rt["a"].queue.front().expect("a request was queued on the agent");
    assert_eq!(req.src, Src::Reply("g".into()));
    assert!(req.text.contains("the checkout page") && req.text.contains("g-r1-a.png"), "{}", req.text);
    assert!(req.text.contains("/bin/weft done"));
    assert_eq!(e.rt["g"].gate.phase, GatePhase::Asking);
    assert!(e.rt["b"].queue.is_empty(), "nothing downstream starts while the gate is open");

    // the agent saves the file and answers; the answer must not fire a's own links
    let path = e.shot_path("g", 1, "a");
    std::fs::create_dir_all(e.shots_dir()).unwrap();
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
    e.rt.get_mut("a").unwrap().reply_to = Some("g".into());
    e.rt.get_mut("a").unwrap().queue.clear();
    e.settle("a", true, "shows the cart with two items".into());
    assert_eq!(e.rt["g"].gate.phase, GatePhase::Review);
    assert_eq!(e.rt["g"].status, Status::Attention);
    assert_eq!(e.rt["g"].gate.shots, vec!["g-r1-a.png".to_string()]);
    assert_eq!(e.rt["a"].output.as_deref(), Some("built the checkout"), "the agent keeps its real result");
    assert!(e.rt["b"].queue.is_empty(), "still blocked until a human confirms");

    // changes requested: back to the agent, new round, still blocked
    e.cmd(None, Cmd::GateChanges { id: "g".into(), text: "make the button blue".into() });
    assert_eq!(e.rt["g"].gate.round, 2);
    assert_eq!(e.rt["g"].gate.phase, GatePhase::Asking);
    let again = e.rt["a"].queue.front().unwrap();
    assert!(again.text.contains("make the button blue") && again.text.contains("g-r2-a.png"), "{}", again.text);
    assert!(e.rt["b"].queue.is_empty());

    // second screenshot arrives and is confirmed: the barrier opens and the image is handed on
    let p2 = e.shot_path("g", 2, "a");
    std::fs::write(&p2, b"\x89PNG\r\n\x1a\n").unwrap();
    e.rt.get_mut("a").unwrap().reply_to = Some("g".into());
    e.rt.get_mut("a").unwrap().queue.clear();
    e.settle("a", true, "button is blue now".into());
    assert_eq!(e.cmd(None, Cmd::Approve { id: "g".into(), ok: true })["ok"], true);
    assert_eq!(e.rt["g"].status, Status::Done);
    let t = &e.rt["b"].queue[0].text;
    assert!(t.contains("Approved screenshot") && t.contains("g-r2-a.png"), "{t}");
}

#[test]
fn a_gate_after_another_gate_still_asks_the_agent_behind_both() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    add_kind(&mut e, "k", "check", "gate", "");
    add_kind(&mut e, "s", "screenshot", "shot", "");
    link(&mut e, "e1", "a", "k", On::Done);
    link(&mut e, "e2", "k", "s", On::Ok);
    e.settle("a", true, "built the checkout".into());
    assert_eq!(e.cmd(None, Cmd::Approve { id: "k".into(), ok: true })["ok"], true);
    assert_eq!(e.rt["s"].gate.targets, vec!["a".to_string()], "the screenshot is asked of the agent, not of the gate");
    let req = e.rt["a"].queue.front().expect("a request was queued on the agent");
    assert_eq!(req.src, Src::Reply("s".into()));
}

#[test]
fn a_missing_screenshot_is_reported_not_silently_accepted() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    add_kind(&mut e, "g", "screenshot", "shot", "");
    link(&mut e, "e1", "a", "g", On::Done);
    e.settle("a", true, "done".into());
    e.rt.get_mut("a").unwrap().reply_to = Some("g".into());
    e.rt.get_mut("a").unwrap().queue.clear();
    e.settle("a", true, "saved it".into()); // but no file exists
    assert_eq!(e.rt["g"].gate.phase, GatePhase::Review);
    assert!(e.rt["g"].gate.shots.is_empty());
    assert!(e.rt["g"].gate.out.contains("No screenshot file appeared"), "{}", e.rt["g"].gate.out);
}

#[test]
fn a_gate_with_nobody_to_ask_hands_the_decision_to_the_human() {
    let mut e = engine();
    add(&mut e, "a", "build ui", "go"); // not running
    add_kind(&mut e, "g", "screenshot", "shot", "");
    link(&mut e, "e1", "a", "g", On::Done);
    e.settle("a", true, "built".into());
    assert_eq!(e.rt["g"].gate.phase, GatePhase::Review);
    assert!(e.rt["g"].gate.out.contains("Paste or drop an image"), "{}", e.rt["g"].gate.out);
}

#[test]
fn a_tests_gate_sends_failures_back_a_few_times_then_asks_the_human() {
    let mut e = engine();
    alive_agent(&mut e, "a", "implement");
    add_kind(&mut e, "t", "tests", "tests", "");
    add(&mut e, "b", "release", "ship {{inputs}}");
    link(&mut e, "e1", "a", "t", On::Done);
    link(&mut e, "e2", "t", "b", On::Ok);
    e.flow.nodes[1].cmd = "make test".into();
    e.flow.nodes[1].retries = 1;
    e.flow.nodes[1].write_tests = false;
    {
        let r = e.rt.get_mut("t").unwrap();
        r.gate.held = Some("held".into());
        r.gate.round = 1;
        r.gate.targets = vec!["a".into()];
    }
    // red: the failure goes to the agent that finished upstream
    e.gate_decide_tests("t", false, "FAIL test_wrap: expected 7 got 6".into());
    assert_eq!(e.rt["t"].gate.round, 2);
    assert_eq!(e.rt["t"].gate.phase, GatePhase::Asking);
    let fix = e.rt["a"].queue.front().unwrap();
    assert!(fix.text.contains("FAIL test_wrap") && fix.src == Src::Reply("t".into()), "{}", fix.text);
    assert!(e.rt["b"].queue.is_empty());
    // red again, retries are used up: the human decides, the barrier stays closed
    e.rt.get_mut("t").unwrap().gate.round = 3;
    e.gate_decide_tests("t", false, "FAIL test_wrap again".into());
    assert_eq!(e.rt["t"].gate.phase, GatePhase::Review);
    assert!(e.rt["b"].queue.is_empty());
    // green: the barrier opens by itself
    e.gate_decide_tests("t", true, "Ran 9 tests\nOK".into());
    assert_eq!(e.rt["t"].status, Status::Done);
    assert!(e.rt["b"].queue[0].text.contains("Tests passed"), "{}", e.rt["b"].queue[0].text);
}

#[test]
fn green_tests_can_still_ask_for_a_human_when_confirm_is_on() {
    let mut e = engine();
    alive_agent(&mut e, "a", "implement");
    add_kind(&mut e, "t", "tests", "tests", "");
    link(&mut e, "e1", "a", "t", On::Done);
    e.flow.nodes[1].cmd = "make test".into();
    e.flow.nodes[1].confirm = true;
    {
        let r = e.rt.get_mut("t").unwrap();
        r.gate.held = Some("held".into());
        r.gate.round = 1;
    }
    e.gate_decide_tests("t", true, "OK".into());
    assert_eq!(e.rt["t"].gate.phase, GatePhase::Review);
    assert_eq!(e.rt["t"].status, Status::Attention);
}

#[test]
fn model_and_reasoning_become_plain_flags() {
    let e = engine();
    let claude = &e.cfg.agents["claude"];
    assert_eq!(tuning_args(claude, "opus", "high"), " --model opus --effort high");
    assert_eq!(tuning_args(claude, "", ""), "", "no choice, no flag");
    assert_eq!(tuning_args(claude, "opus", ""), " --model opus");
    let codex = &e.cfg.agents["codex"];
    assert_eq!(tuning_args(codex, "gpt-5.6-terra", "max"), " -m gpt-5.6-terra -c model_reasoning_effort=max");
    // anything that could change the command line is dropped, not quoted
    assert_eq!(tuning_args(claude, "opus; rm -rf ~", "$(id)"), "");
    // a shell has nothing to choose
    assert_eq!(tuning_args(&e.cfg.agents["sh"], "opus", "high"), "");
}

#[test]
fn model_lists_come_from_the_cli_in_either_shape() {
    // `agy models`: a banner line, then id<TAB>label
    let tabbed =
        "Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\n";
    let l = parse_models(tabbed);
    assert_eq!(l.len(), 2);
    assert_eq!(l[0], ("gemini-3.8-flash-high".to_string(), "Gemini 3.8 Flash (High)".to_string()));
    // `codex debug models`: a JSON catalog; entries the CLI hides stay hidden
    let json = r#"{"models":[
        {"slug":"gpt-6-luna","display_name":"GPT-6-Luna","visibility":"list"},
        {"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide"},
        {"slug":"gpt-5.6-terra","display_name":"GPT-5.6-Terra","visibility":"list"}]}"#;
    let l = parse_models(json);
    assert_eq!(l.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), vec!["gpt-6-luna", "gpt-5.6-terra"]);
    assert_eq!(l[1].1, "GPT-5.6-Terra");
    // bare ids, one per line; junk is ignored
    let l = parse_models("opus\nsonnet\n\nthis line has spaces\n");
    assert_eq!(l.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), vec!["opus", "sonnet"]);
    assert!(parse_models("").is_empty());
    assert!(parse_models("{not json").is_empty());
}

#[test]
fn chosen_model_is_kept_on_the_node_and_prefs_stay_small() {
    let mut e = engine();
    let r = e.cmd(
        None,
        Cmd::NodeAdd { node: json!({"agent": "claude", "model": "opus", "effort": "high", "prompt": "x"}), from: None, run: false },
    );
    let id = r["id"].as_str().unwrap().to_string();
    let n = e.node(&id).unwrap();
    assert_eq!((n.model.as_str(), n.effort.as_str()), ("opus", "high"));
    assert_eq!(e.cmd(None, Cmd::PrefSet { prefs: json!({"agent": "codex", "model": {"codex": "gpt-6-luna"}}) })["ok"], true);
    assert_eq!(e.cfg.prefs["agent"], "codex");
    assert_eq!(e.cmd(None, Cmd::PrefSet { prefs: json!("not an object") })["ok"], false);
    let big = "x".repeat(9000);
    assert_eq!(e.cmd(None, Cmd::PrefSet { prefs: json!({"a": big}) })["ok"], false);
    assert_eq!(e.cfg.prefs["agent"], "codex", "a rejected update changes nothing");
}

#[test]
fn a_folders_check_command_is_guessed_from_its_build_files() {
    let base = std::env::temp_dir().join(format!("weft-check-{}", std::process::id()));
    let at = |name: &str, files: &[(&str, &str)]| {
        let d = base.join(name);
        std::fs::create_dir_all(d.join("tests")).unwrap();
        for (f, body) in files {
            std::fs::write(d.join(f), body).unwrap();
        }
        guess_check(&d)
    };
    assert_eq!(at("rust", &[("Cargo.toml", "")]), "cargo build && cargo test");
    let pkg = r#"{"scripts": {"build": "tsc", "lint": "eslint .", "test": "vitest run", "dev": "vite"}}"#;
    assert_eq!(at("pnpm", &[("package.json", pkg), ("pnpm-lock.yaml", "")]), "pnpm run build && pnpm run lint && pnpm test");
    let npm_default = r#"{"scripts": {"test": "echo \"Error: no test specified\" && exit 1"}}"#;
    assert_eq!(at("npm", &[("package.json", npm_default)]), "", "npm's placeholder test is no test");
    assert_eq!(at("go", &[("go.mod", "module x")]), "go build ./... && go test ./...");
    assert_eq!(at("pytest", &[("pyproject.toml", "[tool.pytest.ini_options]")]), "python3 -m pytest -q");
    assert_eq!(at("unittest", &[("tests/test_x.py", "import unittest")]), "python3 -m unittest discover -s tests -q");
    assert_eq!(at("make", &[("Makefile", "build:\n\tcc x.c\ntest:\n\t./run")]), "make test");
    assert_eq!(at("nothing", &[]), "");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn saved_networks_are_kept_as_given_and_sent_to_the_page() {
    let mut e = engine();
    let net = json!({"id": "my-1", "title": "build and look", "nodes": [
        {"key": "k1", "agent": "@main", "prompt": "{{task}}", "x": 0, "y": 0, "checks": [{"kind": "eta"}]},
        {"key": "k2", "agent": "shot", "x": 650, "y": 0}],
        "links": [{"from": "k1", "to": "k2"}]});
    let networks: Vec<Network> = serde_json::from_value(json!([net])).unwrap();
    assert_eq!(e.cmd(None, Cmd::NetSet { networks })["ok"], true);
    assert_eq!(e.cfg.networks[0].links[0].on, On::Done, "a link fires always unless it says otherwise");
    let st = e.full_state();
    assert_eq!(st["networks"][0]["nodes"][0]["checks"][0]["kind"], "eta", "the cards go back to the page as they came");
    assert_eq!(st["networks"][0]["title"], "build and look");
    let huge = vec![Network { id: "x".into(), title: "x".repeat(600 * 1024), ..Network::default() }];
    assert_eq!(e.cmd(None, Cmd::NetSet { networks: huge })["ok"], false);
    assert_eq!(e.cfg.networks.len(), 1, "a rejected list changes nothing");
}

#[tokio::test]
async fn a_fresh_agent_takes_over_with_a_note_written_by_the_old_one() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.rt.get_mut("a").unwrap().output = Some("first result".into());
    let r = e.cmd(
        None,
        Cmd::Takeover { from: "a".into(), node: json!({"agent": "claude", "x": 400.0, "y": 0.0}), text: "also add tests".into() },
    );
    assert_eq!(r["ok"], true, "{r}");
    let b = r["id"].as_str().unwrap().to_string();
    assert_eq!(e.node(&b).unwrap().takeover, "a");
    assert_eq!(e.node(&b).unwrap().prompt, "also add tests");
    // the old agent is idle: it is asked right away, ahead of anything else it has queued
    assert_eq!(e.rt[&b].hand.phase, HandPhase::Asking);
    assert_eq!(e.rt[&b].status, Status::Waiting);
    let q = &e.rt["a"].queue;
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].src, Src::Reply(b.to_string()));
    assert!(q[0].text.contains("handover note") && q[0].text.contains("done <<'EOF'"), "{}", q[0].text);
    // it answers: its own result stays and nothing fires down its links
    {
        let a = e.rt.get_mut("a").unwrap();
        a.queue.clear();
        a.reply_to = Some(b.clone());
        a.armed = true;
    }
    e.settle("a", true, "goal: ui. files: ui.rs. open: tests".into());
    assert_eq!(e.rt["a"].output.as_deref(), Some("first result"));
    assert_eq!(e.rt[&b].hand.phase, HandPhase::None, "the note is in; the facts are being gathered");
    // the facts arrive: the new agent gets the document and starts
    e.on_hand(&b, "a", Some("goal: ui. files: ui.rs. open: tests".into()), "branch: main\nworking tree: clean".into());
    let item = &e.rt[&b].queue[0];
    assert_eq!(item.src, Src::Flow);
    for want in
        ["also add tests", "Taking over from \"build ui\"", "goal: ui. files: ui.rs", "build it", "State of the repository", "branch: main"]
    {
        assert!(item.text.contains(want), "missing {want:?} in:\n{}", item.text);
    }
    assert!(!item.text.contains("Context from previous steps"), "a handover is not a piped result");
}

#[tokio::test]
async fn a_busy_agent_finishes_its_step_before_it_is_asked_for_a_note() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    {
        let a = e.rt.get_mut("a").unwrap();
        a.status = Status::Busy;
        a.armed = true;
        a.cur = Some(Step::new("build it".into(), vec![]));
    }
    let r = e.cmd(None, Cmd::Takeover { from: "a".into(), node: json!({}), text: String::new() });
    let b = r["id"].as_str().unwrap().to_string();
    assert_eq!(e.rt[&b].hand.phase, HandPhase::Settle);
    assert!(e.rt["a"].queue.is_empty(), "nothing is injected into a running step");
    assert!(e.node(&b).unwrap().prompt.contains("Continue the work"), "{}", e.node(&b).unwrap().prompt);
    // the step ends: the agent settles normally, then the request goes out
    e.rt.get_mut("a").unwrap().cur = None;
    e.settle("a", true, "all built".into());
    assert_eq!(e.rt["a"].output.as_deref(), Some("all built"));
    assert_eq!(e.rt[&b].hand.phase, HandPhase::Asking);
    assert_eq!(e.rt["a"].queue.len(), 1);
    // "don't wait" stops asking and goes with what is on disk
    e.cmd(None, Cmd::Fire { id: b.clone() });
    assert!(e.rt["a"].queue.is_empty());
    assert_eq!(e.rt[&b].hand.phase, HandPhase::None);
}

#[tokio::test]
async fn without_a_running_old_agent_the_takeover_goes_ahead_on_facts_alone() {
    let mut e = engine();
    add(&mut e, "a", "build ui", "build it"); // not running
    e.rt.get_mut("a").unwrap().output = Some("it half works".into());
    let r = e.cmd(None, Cmd::Takeover { from: "a".into(), node: json!({}), text: "finish it".into() });
    let b = r["id"].as_str().unwrap().to_string();
    assert_eq!(e.rt[&b].hand.phase, HandPhase::None);
    e.on_hand(&b, "a", None, "branch: x".into());
    let t = &e.rt[&b].queue[0].text;
    assert!(t.contains("No handover note") && t.contains("it half works") && t.contains("finish it"), "{t}");
}

#[test]
fn what_links_hand_on_is_capped_and_points_at_the_whole_text() {
    let mut e = engine();
    add(&mut e, "a", "writer", "go");
    let long = format!("start {} the end", "x".repeat(30_000));
    let inputs = vec![Input { from: "a".into(), name: "writer".into(), ok: true, text: long, kind: InputKind::Result }];
    let n = node("b", "reader", "summarize");
    let (_, full) = e.render_parts(&n, &inputs);
    assert!(full.len() < 9_500, "{}", full.len());
    assert!(full.contains("the end") && !full.contains("start xxxx"), "the conclusion is kept, the beginning is left out");
    assert!(full.contains("a.out.md"), "{}", &full[..300.min(full.len())]);
}

fn tiny_png(path: &Path) {
    std::fs::write(path, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
}

#[test]
fn a_screenshot_watch_collects_evidence_and_never_blocks_the_agent() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.rt.get_mut("a").unwrap().armed = true; // it is at work
    let r = e.cmd(
        None,
        Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Shot, subject: "the login page".into(), cmd: String::new(), every_s: 0 },
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(e.node("a").unwrap().checks.len(), 1);
    // the agent is told once per session how to show its work, with its next prompt
    assert_eq!(e.take_orders("a"), "", "a running agent was told right when the watch was attached");
    e.rt.get_mut("a").unwrap().checks.told.clear(); // a new session
    let orders = e.take_orders("a");
    assert!(orders.contains("/bin/weft shot <path-to-the-png>") && orders.contains("the login page"), "{orders}");
    assert_eq!(e.take_orders("a"), "", "and not again");
    // a screenshot arrives (as `weft shot` sends it) and is copied next to the others
    let dir = std::env::temp_dir().join(format!("weft-watch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let img = dir.join("login.png");
    tiny_png(&img);
    let r = e.cmd(None, Cmd::FeedFile { node: "a".into(), path: img.to_string_lossy().into(), note: "login form".into() });
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(e.rt["a"].checks.feed.len(), 1);
    assert!(e.rt["a"].checks.feed[0].img.starts_with("a-") && e.rt["a"].checks.feed[0].img.ends_with(".png"));
    assert!(e.shots_dir().join(&e.rt["a"].checks.feed[0].img).is_file());
    // something that is not an image is refused
    let txt = dir.join("notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    let bad = e.cmd(None, Cmd::FeedFile { node: "a".into(), path: txt.to_string_lossy().into(), note: String::new() });
    assert_eq!(bad["ok"], false);
    // approving changes nothing for the agent; steering marks it and keeps what was said
    let id = e.rt["a"].checks.feed[0].id;
    let before = e.rt["a"].status;
    e.cmd(None, Cmd::FeedAct { node: "a".into(), item: id, text: String::new() });
    assert_eq!(e.rt["a"].checks.feed[0].state, FeedState::Ok);
    e.cmd(None, Cmd::FeedAct { node: "a".into(), item: id, text: "the button is cut off".into() });
    assert_eq!(e.rt["a"].checks.feed[0].state, FeedState::Steered);
    assert_eq!(e.rt["a"].checks.feed[0].reply, "the button is cut off");
    assert_eq!(e.rt["a"].status, before, "a watch never changes what the agent is doing");
    // removing the watch removes its evidence
    e.cmd(None, Cmd::CheckDel { node: "a".into(), check: "c1".into() });
    assert!(e.rt["a"].checks.feed.is_empty() && e.node("a").unwrap().checks.is_empty());
    let none = e.cmd(None, Cmd::FeedFile { node: "a".into(), path: img.to_string_lossy().into(), note: String::new() });
    assert_eq!(none["ok"], false, "no watch, no feed");
}

#[tokio::test]
async fn a_watch_with_a_command_runs_it_itself_while_the_agent_works() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let scratch = PathBuf::from(format!("/tmp/weft-unit-watch-{}", std::process::id()));
    let boot = Boot {
        config_file: None,
        dir: scratch.clone(),
        name: "t".into(),
        session: "s".into(),
        sock: "x".into(),
        exe: "/bin/weft".into(),
        url: "http://127.0.0.1:1".into(),
        token: "t".into(),
    };
    let mut e = Engine::new(boot, Config { titles: false, ..Config::default() }, tx, None);
    e.flow_path = scratch.join("flow.json");
    alive_agent(&mut e, "a", "build ui");
    e.cmd(
        None,
        Cmd::CheckAdd {
            node: "a".into(),
            kind: CheckKind::Shot,
            subject: String::new(),
            cmd: "printf '\\211PNG\\r\\n\\032\\n0000000000' > \"$WEFT_SHOT\"; echo front page".into(),
            every_s: 5,
        },
    );
    // an agent that is not working is left alone
    e.rt.get_mut("a").unwrap().status = Status::Idle;
    e.rt.get_mut("a").unwrap().checks.at.insert("c1".into(), 1);
    e.last_check_sec = 0;
    e.tick_checks();
    assert!(!e.rt["a"].checks.busy.contains("c1"));
    // once it works and the interval is up, weft runs the command: nobody is asked
    e.rt.get_mut("a").unwrap().status = Status::Busy;
    e.last_check_sec = 0;
    e.tick_checks();
    assert!(e.rt["a"].checks.busy.contains("c1"));
    assert!(e.rt["a"].queue.is_empty());
    let ev = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Some(ev @ Ev::Check { .. }) => break ev,
                Some(_) => continue,
                None => panic!("channel closed"),
            }
        }
    })
    .await
    .expect("the capture command answered");
    e.handle(ev);
    assert_eq!(e.rt["a"].checks.feed.len(), 1);
    assert_eq!(e.rt["a"].checks.feed[0].note, "front page");
    assert!(e.rt["a"].checks.feed[0].img.ends_with(".png") && !e.rt["a"].checks.busy.contains("c1"));
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_note_for_an_agent_waiting_at_a_dialog_is_held_not_typed() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Shot, subject: String::new(), cmd: String::new(), every_s: 0 });
    e.feed_push("a", "c1", "a-1.png", "front page", None);
    let id = e.rt["a"].checks.feed[0].id;
    // an update menu is on screen: Enter would pick "Update now"
    e.rt.get_mut("a").unwrap().attn = true;
    e.cmd(None, Cmd::FeedAct { node: "a".into(), item: id, text: "move the button".into() });
    let q = &e.rt["a"].queue;
    assert_eq!(q.len(), 1, "held in the queue, which waits for the dialog to clear");
    assert!(q[0].text.contains("move the button"), "{}", q[0].text);
    assert_eq!(e.rt["a"].checks.feed[0].state, FeedState::Steered);
}

#[test]
fn a_worktree_another_agent_works_in_is_not_removed_with_its_owner() {
    let mut e = engine();
    add(&mut e, "a", "build", "go");
    add(&mut e, "b", "takes over", "go on");
    e.rt.get_mut("a").unwrap().worktree = Some("/tmp/weft-unit-wt-a".into());
    assert!(!e.worktree_shared("a"));
    e.flow.nodes.iter_mut().find(|n| n.id == "b").unwrap().cwd = "/tmp/weft-unit-wt-a".into();
    assert!(e.worktree_shared("a"), "b works in a's worktree: deleting a must leave it");
}

#[test]
fn agents_run_in_yolo_mode_unless_switched_off() {
    let e = engine();
    let claude = &e.cfg.agents["claude"];
    let mut n = node("n", "x", "go");
    n.agent = "claude".into();
    // the default: on, and the alias is skipped so the flag appears exactly once
    assert_eq!(agent_command(claude, &n, true), "command claude --dangerously-skip-permissions");
    // a card that switched it off, or a config that did
    n.yolo = Some(false);
    assert_eq!(agent_command(claude, &n, true), "command claude");
    n.yolo = None;
    assert_eq!(agent_command(claude, &n, false), "command claude");
    n.yolo = Some(true);
    assert_eq!(agent_command(claude, &n, false), "command claude --dangerously-skip-permissions");
    // model and reasoning ride along
    n.model = "opus".into();
    n.effort = "high".into();
    assert_eq!(agent_command(claude, &n, true), "command claude --dangerously-skip-permissions --model opus --effort high");
    let codex = &e.cfg.agents["codex"];
    n.agent = "codex".into();
    n.model = String::new();
    assert_eq!(agent_command(codex, &n, true), "command codex --dangerously-bypass-approvals-and-sandbox -c model_reasoning_effort=high");
    // a flag already in the configured command is not repeated (codex refuses that)
    let mut own = codex.clone();
    own.cmd = "codex --dangerously-bypass-approvals-and-sandbox".into();
    n.effort = String::new();
    assert_eq!(agent_command(&own, &n, true), "command codex --dangerously-bypass-approvals-and-sandbox");
    // an agent weft does not manage keeps its command line exactly
    let aider = AgentCfg {
        cmd: "aider --yes".into(),
        label: String::new(),
        attention: vec![],
        model_arg: String::new(),
        models: vec![],
        models_cmd: String::new(),
        effort_arg: String::new(),
        efforts: vec![],
        yolo_arg: String::new(),
        working: vec![],
        busy: vec![],
        effort_models: vec![],
        title_cmd: String::new(),
    };
    assert_eq!(agent_command(&aider, &n, true), "aider --yes");
    // environment assignments in front keep working: no `command` prefix, the flag still goes on
    let mut env = claude.clone();
    env.cmd = "FOO=1 claude".into();
    n.agent = "claude".into();
    assert_eq!(agent_command(&env, &n, true), "FOO=1 claude --dangerously-skip-permissions");
    // flow.json stays tidy: no yolo key unless a card decided
    let v = serde_json::to_value(node("m", "y", "go")).unwrap();
    assert!(v.get("yolo").is_none());
}

#[test]
fn an_agent_that_announced_background_work_is_not_finished() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.flow.nodes[0].agent = "claude".into();
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.status = Status::Busy;
        r.cur = Some(Step::new("go".into(), vec![]));
        // what Claude Code shows after a turn that started a shell it will wait for
        r.parser.process("\u{25cf} I started the sleep in the background and will answer again when it finishes.\r\n\u{273b} Brewed for 3s \u{b7} done 7:07 \u{b7} 1 shell still running\r\n".as_bytes());
        r.dirty = true;
    }
    let t0 = Instant::now();
    e.eval("a", t0);
    assert!(e.rt["a"].work);
    // idle for far longer than the quiet time: still waiting for the shell
    e.eval("a", t0 + Duration::from_secs(30));
    assert!(!e.rt["a"].finished, "it said it will answer later");
    assert_eq!(e.rt["a"].status, Status::Busy);
    assert!(e.rt["a"].bg);
    // the shell ends and the agent answers: the marker is gone, the screen goes quiet, now it is done
    {
        let r = e.rt.get_mut("a").unwrap();
        r.parser.process("\x1b[2J\x1b[H\u{25cf} The sleep finished.\r\n".as_bytes());
        r.dirty = true;
    }
    e.eval("a", t0 + Duration::from_secs(31));
    assert!(!e.rt["a"].work && !e.rt["a"].bg);
    e.eval("a", t0 + Duration::from_secs(40));
    assert!(e.rt["a"].finished, "quiet and nothing running: done");
}

#[test]
fn waiting_for_background_work_is_capped() {
    let mut e = engine();
    e.cfg.background_s = 60.0;
    alive_agent(&mut e, "a", "build ui");
    e.flow.nodes[0].agent = "claude".into();
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.status = Status::Busy;
        r.cur = Some(Step::new("go".into(), vec![]));
        r.parser.process("\u{273b} Worked for 2s \u{b7} done 7:07 \u{b7} 1 shell still running\r\n".as_bytes());
        r.dirty = true;
    }
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(50));
    assert!(!e.rt["a"].finished);
    // a server it left running must not hold the flow for ever
    e.eval("a", t0 + Duration::from_secs(70));
    e.eval("a", t0 + Duration::from_secs(80));
    assert!(e.rt["a"].finished, "after the cap it counts as finished");
}

#[test]
fn a_step_that_gave_its_last_word_is_not_held_by_what_it_left_running() {
    // a server, or a wait loop that never ends: Claude Code shows the shell as long as it runs
    let running = "\u{273b} Brewed for 1m \u{b7} done 19:04 \u{b7} 1 shell still running";
    let t0 = Instant::now();
    let mut e = engine();
    chained_agent(&mut e, &format!("Set up the repository.\r\nSTATUS: DONE\r\n\r\n{running}"));
    e.flow.nodes[0].agent = "claude".into();
    e.eval("a", t0);
    assert!(e.rt["a"].work && !e.rt["a"].bg);
    e.eval("a", t0 + Duration::from_secs(15));
    assert!(e.rt["a"].finished, "it said DONE: the shell is not the step");
    assert!(e.rt["b"].queue.iter().any(|q| q.text.contains("Set up the repository")));
    // one that waits for that shell is held by it
    let mut e = engine();
    chained_agent(&mut e, &format!("Started the build.\r\nSTATUS: WAITING for the build\r\n\r\n{running}"));
    e.flow.nodes[0].agent = "claude".into();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(30));
    assert!(e.rt["a"].bg && !e.rt["a"].finished);
}

#[test]
fn messages_typed_to_an_agent_carry_its_addenda_unless_they_are_commands() {
    let mut e = engine();
    let mut n = node("a", "build ui", "go");
    n.addenda = vec!["report-done".into()];
    e.flow.nodes.push(n);
    e.rt.insert("a".into(), Rt::new(80, 24));
    let n = e.node("a").unwrap().clone();
    let full = e.with_addenda(&n, "wait another 120 seconds before answering");
    assert!(full.starts_with("wait another 120 seconds before answering"));
    assert!(full.contains("/bin/weft done <<'EOF'"), "{full}");
    assert_eq!(e.with_addenda(&n, "/compact"), "/compact");
    assert_eq!(e.with_addenda(&n, "!ls"), "!ls");
    // a shell command takes its input as typed
    let mut sh = n.clone();
    sh.finish = Finish::Exit;
    assert_eq!(e.with_addenda(&sh, "ls"), "ls");
}

#[test]
fn agy_takes_a_reasoning_level_only_with_its_default_model() {
    let e = engine();
    let agy = &e.cfg.agents["agy"];
    assert_eq!(agy.efforts, vec!["low", "medium", "high"]);
    assert_eq!(tuning_args(agy, "", "low"), " --effort low");
    // a model with the level in its name, or one that has none: agy would drop the whole choice
    assert_eq!(tuning_args(agy, "gemini-3.8-flash-medium", "low"), " --model gemini-3.8-flash-medium");
    assert_eq!(tuning_args(agy, "claude-opus-4-6-thinking", "high"), " --model claude-opus-4-6-thinking");
    // the other agents take it with any model
    let claude = &e.cfg.agents["claude"];
    assert_eq!(tuning_args(claude, "opus", "max"), " --model opus --effort max");
}

#[test]
fn an_eta_is_asked_for_once_counted_down_and_cleared_when_the_agent_finishes() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.rt.get_mut("a").unwrap().armed = true; // it is at work
    // nobody has been asked, and the CLI call is refused without the check
    let no = e.cmd(None, Cmd::Eta { node: "a".into(), seconds: 90, note: String::new() });
    assert_eq!(no["ok"], false);
    let r = e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Eta, subject: String::new(), cmd: String::new(), every_s: 0 });
    assert_eq!(r["ok"], true, "{r}");
    // a second drop keeps the one that is there
    let again =
        e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Eta, subject: String::new(), cmd: String::new(), every_s: 0 });
    assert_eq!(again["check"], r["check"]);
    assert_eq!(e.node("a").unwrap().checks.len(), 1);
    // the standing order was queued for the running agent, once
    assert_eq!(e.take_orders("a"), "");
    e.rt.get_mut("a").unwrap().checks.told.clear();
    assert!(e.take_orders("a").contains("eta <seconds>"));
    // the agent answers: a deadline in the future
    let before = now_ms();
    let ok = e.cmd(None, Cmd::Eta { node: "a".into(), seconds: 120, note: "plan: read, fix, test".into() });
    assert_eq!(ok["ok"], true);
    let eta = e.rt["a"].checks.eta.clone().unwrap();
    assert!(eta.at >= before + 119_000 && eta.at <= now_ms() + 121_000, "{}", eta.at);
    assert_eq!(eta.total, 120);
    assert_eq!(eta.note, "plan: read, fix, test");
    // a new estimate replaces it
    e.cmd(None, Cmd::Eta { node: "a".into(), seconds: 30, note: "almost".into() });
    assert_eq!(e.rt["a"].checks.eta.as_ref().unwrap().total, 30);
    // a finished agent has no countdown
    e.rt.get_mut("a").unwrap().armed = true;
    e.settle("a", true, "done".into());
    assert!(e.rt["a"].checks.eta.is_none());
    // and a late estimate is ignored
    let late = e.cmd(None, Cmd::Eta { node: "a".into(), seconds: 50, note: String::new() });
    assert_eq!(late["ok"], true);
    assert!(e.rt["a"].checks.eta.is_none());
}

#[test]
fn an_estimate_that_ran_out_is_asked_for_again_once() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Eta, subject: String::new(), cmd: String::new(), every_s: 0 });
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.status = Status::Busy;
        r.run_ms = now_ms() - 5 * 60_000;
        r.checks.eta = Some(Eta { at: now_ms() - 20_000, set: now_ms() - 200_000, total: 180, note: String::new(), asked: 0 });
        r.queue.clear();
    }
    e.tick_eta(now_ms());
    // no tmux in a unit test: the request is held in the queue, which is where a dialog would put it too
    e.rt.get_mut("a").unwrap().attn = false;
    assert!(e.rt["a"].checks.eta.as_ref().unwrap().asked > 0, "asked once");
    let asked = e.rt["a"].checks.eta.as_ref().unwrap().asked;
    e.tick_eta(now_ms() + 1_000);
    assert_eq!(e.rt["a"].checks.eta.as_ref().unwrap().asked, asked, "not nagging");
}

#[test]
fn the_changes_watch_keeps_one_live_item() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    let r =
        e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Changes, subject: String::new(), cmd: String::new(), every_s: 0 });
    assert_eq!(r["ok"], true);
    assert_eq!(e.node("a").unwrap().checks[0].every_s, 20, "looked at every 20 s by default");
    e.on_check("a", "c1", CheckKind::Changes, true, "1 file \u{b7} +3 \u{2212}1\nM a.py  +3 \u{2212}1", "");
    e.on_check("a", "c1", CheckKind::Changes, true, "1 file \u{b7} +3 \u{2212}1\nM a.py  +3 \u{2212}1", "");
    assert_eq!(e.rt["a"].checks.feed.len(), 1, "the same state is not a new item");
    e.rt.get_mut("a").unwrap().checks.feed[0].state = FeedState::Ok;
    e.on_check("a", "c1", CheckKind::Changes, true, "2 files \u{b7} +5 \u{2212}1\nM a.py  +3 \u{2212}1\n? b.py", "");
    assert_eq!(e.rt["a"].checks.feed.len(), 1, "it is replaced, not piled up");
    assert_eq!(e.rt["a"].checks.feed[0].state, FeedState::New, "new content wants a new look");
    assert!(e.rt["a"].checks.feed[0].note.starts_with("2 files"));
}

#[test]
fn a_queued_prompt_is_answered_by_the_same_agent_and_its_answer_flows_on() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    {
        let r = e.rt.get_mut("a").unwrap();
        r.finished = true;
        r.armed = true;
        r.ok = Some(true);
        r.output = Some("the first result".into());
    }
    // the step: its prompt is asked of "a" in a's own session
    let mut st = node("s", "more", "also add tests");
    st.agent = "step".into();
    st.host = "a".into();
    e.flow.nodes.push(st);
    e.rt.insert("s".into(), Rt::new(80, 24));
    add(&mut e, "b", "review", "review it");
    link(&mut e, "e1", "a", "s", On::Done);
    link(&mut e, "e2", "s", "b", On::Done);
    e.add_edge("a", "s", On::Done).unwrap(); // a is settled: its result is delivered at once
    let q = &e.rt["a"].queue;
    assert_eq!(q.len(), 1, "the prompt waits in a's own queue");
    assert_eq!(q[0].src, Src::Reply("s".into()));
    assert!(q[0].text.contains("also add tests") && !q[0].text.contains("the first result"), "{}", q[0].text);
    assert_eq!(e.rt["s"].status, Status::Busy);
    assert_eq!(e.rt["s"].step_wait.as_deref(), Some("a"));
    // a answers it: that is the step's result, a's own result is untouched and a's links stay quiet
    {
        let a = e.rt.get_mut("a").unwrap();
        a.queue.clear();
        a.reply_to = Some("s".into());
    }
    e.settle("a", true, "tests added".into());
    assert_eq!(e.rt["s"].output.as_deref(), Some("tests added"));
    assert_eq!(e.rt["s"].status, Status::Done);
    assert_eq!(e.rt["a"].output.as_deref(), Some("the first result"));
    // and it goes on from the step
    assert!(
        e.rt["b"].queue.iter().any(|q| q.text.contains("tests added")),
        "{:?}",
        e.rt["b"].queue.iter().map(|q| q.text.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn messages_to_a_queued_prompt_go_to_its_agent() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    let mut st = node("s", "more", "x");
    st.agent = "step".into();
    st.host = "a".into();
    e.flow.nodes.push(st);
    assert_eq!(e.resolve_host("s"), "a");
    assert_eq!(e.resolve_host("a"), "a");
    assert_eq!(e.resolve_host("nobody"), "nobody");
}

#[test]
fn a_commit_resolves_when_its_agents_have_committed_and_goes_on() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build ui");
    add_kind(&mut e, "c", "commit", "git", "");
    e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().git_op = GitOp::Commit;
    link(&mut e, "e1", "a", "c", On::Done);
    e.rt.get_mut("c").unwrap().git.seen = vec!["e1".into()];
    e.git_pump("c", false);
    assert_eq!(e.rt["c"].status, Status::Done, "all in, nothing to push: the commit step is resolved");
    assert!(e.rt["c"].finished);
}

#[tokio::test]
async fn a_push_waits_until_every_commit_before_it_has_resolved() {
    let mut e = engine();
    add_kind(&mut e, "c1", "commit one", "git", "");
    add_kind(&mut e, "c2", "commit two", "git", "");
    add_kind(&mut e, "p", "push", "git", "");
    for (id, op) in [("c1", "commit"), ("c2", "commit"), ("p", "push")] {
        let n = e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap();
        n.git_op = serde_json::from_value(json!(op)).unwrap();
        n.draft = false;
    }
    link(&mut e, "e1", "c1", "p", On::Done);
    link(&mut e, "e2", "c2", "p", On::Done);
    // the first commit resolves: the push still waits for the other
    e.rt.get_mut("c1").unwrap().git.seen = vec!["x".into()];
    e.settle("c1", true, "abc123 first".into());
    assert_eq!(e.rt["p"].git.seen.len(), 1);
    assert!(!e.rt["p"].git.finishing, "one commit of two is not enough");
    // the second does: now it pushes
    e.rt.get_mut("c2").unwrap().git.seen = vec!["y".into()];
    e.settle("c2", true, "def456 second".into());
    assert_eq!(e.rt["p"].git.seen.len(), 2);
    assert!(e.rt["p"].git.finishing && e.rt["p"].git.busy, "both resolved: the push runs");
}

#[tokio::test]
async fn a_failed_commit_stops_the_push() {
    let mut e = engine();
    add_kind(&mut e, "c", "commit", "git", "");
    add_kind(&mut e, "p", "push", "git", "");
    for (id, op) in [("c", "commit"), ("p", "push")] {
        let n = e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap();
        n.git_op = serde_json::from_value(json!(op)).unwrap();
        n.draft = false;
    }
    link(&mut e, "e1", "c", "p", On::Done);
    e.rt.get_mut("c").unwrap().git.seen = vec!["x".into()];
    e.settle("c", false, "failed: agent could not commit".into());
    assert!(!e.rt["p"].git.finishing);
    assert_eq!(e.rt["p"].status, Status::Failed);
    assert!(e.rt["p"].git.log.iter().any(|l| l.contains("nothing to push")), "{:?}", e.rt["p"].git.log);
}
fn git_node(e: &mut Engine, id: &str, name: &str, op: &str) {
    add_kind(e, id, name, "git", "");
    let n = e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap();
    n.git_op = serde_json::from_value(json!(op)).unwrap();
    n.draft = n.git_op != GitOp::Commit;
}

#[test]
fn an_agent_that_failed_is_not_committed() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    git_node(&mut e, "c", "commit", "commit");
    e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().draft = false;
    link(&mut e, "e1", "a", "c", On::Done);
    e.git_arrival("c", "e1", "a", "slug", false, "could not finish");
    assert_eq!(e.rt["c"].status, Status::Failed);
    assert!(e.rt["c"].git.log.iter().any(|l| l.contains("did not finish its work")), "{:?}", e.rt["c"].git.log);
    assert!(e.rt["a"].queue.is_empty(), "the failed agent is not asked to commit");
}

#[test]
fn a_commit_takes_the_work_of_one_agent() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    alive_agent(&mut e, "b", "wrap");
    git_node(&mut e, "c", "commit", "commit");
    git_node(&mut e, "p", "push", "push");
    assert!(e.add_edge("a", "c", On::Done).is_ok());
    assert!(e.add_edge("a", "c", On::Ok).is_ok(), "the same link may be set again");
    let err = e.add_edge("b", "c", On::Done).unwrap_err();
    assert!(err.contains("one agent"), "{err}");
    // the push is where they meet
    assert!(e.add_edge("a", "p", On::Done).is_ok());
    assert!(e.add_edge("b", "p", On::Done).is_ok());
    assert_eq!(e.flow.edges.iter().filter(|x| x.to == "p").count(), 2);
    // and a node with several links in cannot be turned into a commit
    let r = e.cmd(None, Cmd::NodeSet { id: "p".into(), patch: json!({"git_op": "commit"}) });
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn a_gate_between_the_agent_and_the_commit_still_asks_that_agent() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    add_kind(&mut e, "g", "approve", "gate", "");
    git_node(&mut e, "c", "commit", "commit");
    e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().draft = false;
    link(&mut e, "e1", "a", "g", On::Done);
    link(&mut e, "e2", "g", "c", On::Ok);
    e.rt.get_mut("g").unwrap().gate.targets = vec!["a".into()];
    e.settle("g", true, "### Output of \"slug\"\nfixed it".into());
    let q: Vec<String> = e.rt["a"].queue.iter().map(|q| q.text.clone()).collect();
    assert!(q.iter().any(|t| t.contains("Commit the work you just did")), "the agent behind the gate is asked: {q:?}");
    assert!(e.rt["a"].queue.iter().any(|q| q.src == Src::Reply("c".into())));
    assert_eq!(e.rt["c"].git.carry.len(), 1);
    assert_eq!(e.rt["c"].git.carry[0].name, "slug", "credited to the agent, not to the gate");
}

#[test]
fn a_commit_hands_on_a_report_of_what_was_done() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "claude".into();
    e.rt.get_mut("a").unwrap().asked = "Fix the slugify function so the tests pass".into();
    git_node(&mut e, "c", "commit", "commit");
    e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().draft = false;
    add(&mut e, "t", "tester", "test the features live");
    e.flow.nodes.iter_mut().find(|n| n.id == "t").unwrap().draft = false;
    link(&mut e, "e1", "a", "c", On::Done);
    link(&mut e, "e2", "c", "t", On::Done);
    e.git_arrival("c", "e1", "a", "slug", true, "slugify now lowercases and strips accents.");
    assert!(e.rt["a"].queue.iter().any(|q| q.src == Src::Reply("c".into())), "the agent is asked to commit");
    // the agent committed; git says what happened
    let facts = Facts {
        branch: "feature".into(),
        head: "a1b2c3d Fix slug".into(),
        commits: vec!["a1b2c3d Fix slug".into()],
        files: vec!["textkit/slug.py".into()],
        ..Default::default()
    };
    e.on_git("c".into(), 0, GitJobKind::Commit, Ok("a1b2c3d Fix slug (slug: done)".into()), Some(facts));
    assert_eq!(e.rt["c"].status, Status::Done);
    let out = e.rt["c"].output.clone().unwrap();
    for want in [
        "Committed on `feature`",
        "a1b2c3d Fix slug",
        "Files changed (1): textkit/slug.py",
        "- slug (",
        "was asked: Fix the slugify function",
        "slugify now lowercases",
        "commit: a1b2c3d",
    ] {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
    // and the agent linked after it starts from exactly that
    let q: Vec<String> = e.rt["t"].queue.iter().map(|q| q.text.clone()).collect();
    assert!(
        q.iter().any(|t| t.contains("Committed on `feature`") && t.contains("textkit/slug.py") && t.contains("test the features live")),
        "{q:?}"
    );
}

#[tokio::test]
async fn the_push_carries_every_commit_and_every_agent_to_what_follows() {
    let mut e = engine();
    git_node(&mut e, "c1", "commit slug", "commit");
    git_node(&mut e, "c2", "commit wrap", "commit");
    git_node(&mut e, "p", "push", "push");
    e.flow.nodes.iter_mut().find(|n| n.id == "p").unwrap().draft = false;
    link(&mut e, "e1", "c1", "p", On::Done);
    link(&mut e, "e2", "c2", "p", On::Done);
    let carry = |name: &str, node: &str, task: &str, commit: &str| Carry {
        name: name.into(),
        agent: "claude".into(),
        node: node.into(),
        task: task.into(),
        result: format!("{name} is done"),
        commit: commit.into(),
    };
    e.rt.get_mut("c1").unwrap().git.carry = vec![carry("slug", "a", "fix slug", "a1b2c3d Fix slug")];
    e.rt.get_mut("c1").unwrap().git.facts = Facts {
        branch: "feature".into(),
        head: "a1b2c3d Fix slug".into(),
        commits: vec!["a1b2c3d Fix slug".into()],
        files: vec!["slug.py".into()],
        ..Default::default()
    };
    e.rt.get_mut("c2").unwrap().git.carry = vec![carry("wrap", "b", "fix wrap", "d4e5f6a Fix wrap")];
    e.rt.get_mut("c2").unwrap().git.facts = Facts {
        branch: "feature".into(),
        head: "d4e5f6a Fix wrap".into(),
        commits: vec!["d4e5f6a Fix wrap".into()],
        files: vec!["wrap.py".into()],
        ..Default::default()
    };
    e.rt.get_mut("c1").unwrap().git.seen = vec!["x".into()];
    e.settle("c1", true, "report one".into());
    assert!(!e.rt["p"].git.finishing);
    e.rt.get_mut("c2").unwrap().git.seen = vec!["y".into()];
    e.settle("c2", true, "report two".into());
    assert!(e.rt["p"].git.finishing, "both commits are in: the push runs");
    let names: Vec<&str> = e.rt["p"].git.carry.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["slug", "wrap"]);
    assert_eq!(e.rt["p"].git.facts.files, vec!["slug.py".to_string(), "wrap.py".to_string()]);
    assert_eq!(carry_hashes(&e.rt["p"].git.carry), vec!["a1b2c3d".to_string(), "d4e5f6a".to_string()]);
    // the push finishes: its result is the report with both
    e.rt.get_mut("p").unwrap().git.busy = true;
    let f = Facts {
        branch: "feature".into(),
        head: "d4e5f6a Fix wrap".into(),
        commits: vec!["d4e5f6a Fix wrap".into(), "a1b2c3d Fix slug".into()],
        ..Default::default()
    };
    e.on_git("p".into(), 0, GitJobKind::Push, Ok("pushed feature to origin".into()), Some(f));
    let out = e.rt["p"].output.clone().unwrap();
    assert!(out.starts_with("Pushed `feature` to `origin`."), "{out}");
    for want in
        ["- slug (claude)", "- wrap (claude)", "was asked: fix slug", "commit: d4e5f6a Fix wrap", "Files changed (2): slug.py, wrap.py"]
    {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
}

#[test]
fn a_pull_request_is_opened_by_the_agent_that_knows_the_work() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    git_node(&mut e, "p", "push", "push");
    git_node(&mut e, "r", "open PR", "pr");
    e.flow.nodes.iter_mut().find(|n| n.id == "r").unwrap().draft = false;
    e.flow.nodes.iter_mut().find(|n| n.id == "r").unwrap().prompt = "label it agent-made".into();
    add(&mut e, "t", "reviewer", "review the PR");
    e.flow.nodes.iter_mut().find(|n| n.id == "t").unwrap().draft = false;
    link(&mut e, "e1", "p", "r", On::Done);
    link(&mut e, "e2", "r", "t", On::Done);
    // what the push before it passes on
    e.rt.get_mut("p").unwrap().git.carry = vec![Carry {
        name: "slug".into(),
        agent: "claude".into(),
        node: "a".into(),
        task: "fix slug".into(),
        result: "done".into(),
        commit: "a1b2c3d Fix slug".into(),
    }];
    e.rt.get_mut("p").unwrap().git.facts = Facts {
        branch: "feature".into(),
        head: "a1b2c3d Fix slug".into(),
        commits: vec!["a1b2c3d Fix slug".into()],
        files: vec!["slug.py".into()],
        ..Default::default()
    };
    e.rt.get_mut("p").unwrap().git.seen = vec!["x".into()];
    e.settle("p", true, "pushed".into());
    // nothing to push again: the agent is asked right away
    let q: Vec<String> = e.rt["a"].queue.iter().map(|q| q.text.clone()).collect();
    assert!(
        q.iter().any(|t| t.contains("open a pull request")
            && t.contains("`feature`")
            && t.contains("label it agent-made")
            && t.contains("a1b2c3d Fix slug")),
        "{q:?}"
    );
    assert!(!e.rt["r"].git.log.iter().any(|l| l.contains("pushed")), "it did not push a second time");
    assert_eq!(e.rt["r"].git.stage, GitStage::Pr);
    // the agent answers with the link
    e.git_response("r", "a", true, "https://github.com/acme/textkit/pull/7\nFix slugify and wrap");
    assert_eq!(e.rt["r"].status, Status::Done);
    let out = e.rt["r"].output.clone().unwrap();
    assert!(
        out.starts_with("Opened a pull request for `feature`: https://github.com/acme/textkit/pull/7\nTitle: Fix slugify and wrap"),
        "{out}"
    );
    // and the reviewer after it gets the link and the facts
    let q: Vec<String> = e.rt["t"].queue.iter().map(|q| q.text.clone()).collect();
    assert!(q.iter().any(|t| t.contains("pull/7") && t.contains("review the PR")), "{q:?}");
}

#[test]
fn a_pull_request_nobody_can_write_fails_with_a_reason() {
    let mut e = engine();
    git_node(&mut e, "p", "push", "push");
    git_node(&mut e, "r", "open PR", "pr");
    e.flow.nodes.iter_mut().find(|n| n.id == "r").unwrap().draft = false;
    link(&mut e, "x", "p", "r", On::Done);
    e.rt.get_mut("r").unwrap().git.seen = vec!["x".into()];
    // the push before it is done, but none of the agents that did the work is running any more
    e.git_pump("r", true);
    assert_eq!(e.rt["r"].status, Status::Failed);
    assert!(e.rt["r"].git.log.iter().any(|l| l.contains("no agent")), "{:?}", e.rt["r"].git.log);
}

#[tokio::test]
async fn a_rejected_push_has_the_agent_bring_the_branch_up_to_date_once() {
    let mut e = engine();
    alive_agent(&mut e, "a", "slug");
    git_node(&mut e, "p", "push", "push");
    e.flow.nodes.iter_mut().find(|n| n.id == "p").unwrap().draft = false;
    e.rt.get_mut("p").unwrap().git.seen = vec!["x".into()];
    e.rt.get_mut("p").unwrap().git.carry =
        vec![Carry { name: "slug".into(), agent: "claude".into(), node: "a".into(), ..Default::default() }];
    e.rt.get_mut("p").unwrap().git.busy = true;
    e.rt.get_mut("p").unwrap().git.finishing = true;
    e.on_git("p".into(), 0, GitJobKind::Push, Err("REJECTED:push failed: ! [rejected] feature -> feature (fetch first)".into()), None);
    assert_eq!(e.rt["p"].git.stage, GitStage::Rebase);
    let q: Vec<String> = e.rt["a"].queue.iter().map(|q| q.text.clone()).collect();
    assert!(q.iter().any(|t| t.contains("was rejected") && t.contains("git pull --rebase") && t.contains("Do not force-push")), "{q:?}");
    assert_ne!(e.rt["p"].status, Status::Failed);
    // it reports back: the push is tried again
    e.git_response("p", "a", true, "rebased onto origin/feature");
    assert!(e.rt["p"].git.finishing && e.rt["p"].git.busy, "pushing again");
    // a second rejection is final
    e.on_git("p".into(), 0, GitJobKind::Push, Err("REJECTED:push failed: still rejected".into()), None);
    assert_eq!(e.rt["p"].status, Status::Failed);
    assert!(e.rt["p"].git.log.iter().any(|l| l.contains("still rejected")));
}

#[test]
fn a_pull_request_link_is_found_in_what_the_agent_says() {
    let (u, t) = pr_link("Opened it: https://github.com/a/b/pull/12.\nFix the slug").unwrap();
    assert_eq!(u, "https://github.com/a/b/pull/12");
    assert_eq!(t, "Fix the slug");
    let (u, _) = pr_link("see <https://gitlab.com/g/p/-/merge_requests/3> for details").unwrap();
    assert_eq!(u, "https://gitlab.com/g/p/-/merge_requests/3");
    // a link that is not a pull request is better than none, but a pull request wins
    let (u, _) = pr_link("docs https://example.com/readme\nPR: https://github.com/a/b/pull/9").unwrap();
    assert_eq!(u, "https://github.com/a/b/pull/9");
    assert!(pr_link("done, no link").is_none());
}

#[test]
fn ending_stops_every_agent_and_nothing_starts_after_it() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "go");
    e.rt.get_mut("n1").unwrap().alive = true;
    let r = e.cmd_quit(true);
    assert_eq!(r["agents"], 1, "only the agent that runs is counted");
    assert!(e.exit == Some(Exit::End));
    assert!(!e.rt["n1"].alive);
    assert_eq!(e.rt["n1"].status, Status::Stopped);
    // a prompt that arrives now must not bring a process back: the session is about to go
    e.start("n2");
    assert!(!e.tmux_retrying && e.spawns == 0);
}

#[test]
fn keeping_leaves_the_agents_alone() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    e.rt.get_mut("n1").unwrap().alive = true;
    let r = e.cmd_quit(false);
    assert_eq!((r["agents"].as_u64(), r["end"].as_bool()), (Some(1), Some(false)));
    assert!(e.exit == Some(Exit::Keep));
    assert!(e.rt["n1"].alive, "the agent keeps running in tmux");
}

#[test]
fn what_was_queued_for_a_process_that_is_gone_waits_as_stopped() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add(&mut e, "n2", "b", "go");
    e.enqueue("n1", "do it".into(), "do it".into(), Src::Flow, false);
    for (id, want) in [("n1", Status::Stopped), ("n2", Status::New)] {
        let mut r = Rt::new(80, 24);
        r.restore(e.rt[id].to_saved());
        assert_eq!(r.status, want, "{id}");
    }
}

/// An agent mid-step whose screen says it will answer later.
fn deferring_agent(e: &mut Engine, step: Step) {
    alive_agent(e, "a", "build");
    let r = e.rt.get_mut("a").unwrap();
    r.armed = true;
    r.status = Status::Busy;
    r.cur = Some(step);
    r.parser.process(b"I will get back to you when the build is done.\r\n");
    r.dirty = true;
}

fn with_command() -> Step {
    Step { expect_signal: true, ..Step::new("go".into(), vec![]) }
}

/// A step of a running chain: it was asked to end with a status line, and something follows it.
fn chain_step() -> Step {
    Step { rule: true, ..Step::new("go".into(), vec![]) }
}

fn chained_agent(e: &mut Engine, said: &str) {
    deferring_agent(e, chain_step());
    e.rt.get_mut("a").unwrap().parser.process(format!("\x1b[2J\x1b[H{said}\r\n").as_bytes());
    add(e, "b", "next", "{{inputs}}");
    link(e, "e1", "a", "b", On::Done);
    e.rt.get_mut("a").unwrap().cause = Cause::Flow;
}

fn says(e: &mut Engine, text: &str) {
    let r = e.rt.get_mut("a").unwrap();
    r.parser.process(format!("{text}\r\n").as_bytes());
    r.dirty = true;
}

#[test]
fn silence_after_a_command_request_is_asked_about_once_before_it_counts() {
    let mut e = engine();
    deferring_agent(&mut e, with_command());
    let t0 = Instant::now();
    e.eval("a", t0);
    // inside the grace period nothing happens
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(!e.rt["a"].finished && !e.rt["a"].cur.as_ref().unwrap().asked);
    // a promise to get back is not a report: the agent is reminded, the step stays open
    e.eval("a", t0 + Duration::from_secs(20));
    assert!(!e.rt["a"].finished, "the deferral must not finish the step");
    assert!(e.rt["a"].cur.as_ref().unwrap().asked);
    // silent again after the reminder: now it is the end, so a flow never hangs on an agent that does not report
    e.eval("a", t0 + Duration::from_secs(40));
    assert!(e.rt["a"].finished);
}

#[test]
fn an_agent_that_was_never_asked_to_report_is_finished_by_silence_alone() {
    let mut e = engine();
    deferring_agent(&mut e, Step::new("go".into(), vec![]));
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(e.rt["a"].finished, "nothing was asked of it, so there is nothing to wait for");
}

#[test]
fn a_status_line_at_the_end_decides_how_a_chained_step_ends() {
    // DONE: what it said goes on, without the label
    let mut e = engine();
    chained_agent(&mut e, "Fixed slug.py and ran the tests: all green.\nSTATUS: DONE");
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(e.rt["a"].finished && e.rt["a"].ok == Some(true));
    assert_eq!(e.rt["a"].output.as_deref(), Some("Fixed slug.py and ran the tests: all green."));
    assert!(
        e.rt["b"].queue.iter().any(|q| q.text.contains("all green") && !q.text.contains("STATUS")),
        "the next step got the text, not the label"
    );

    // FAILED: the failure links fire, with the reason first
    let mut e = engine();
    chained_agent(&mut e, "I tried three ways.\nSTATUS: FAILED the tests need a database that is not available here");
    link(&mut e, "e2", "a", "b", On::Fail);
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert_eq!(e.rt["a"].ok, Some(false));
    let out = e.rt["a"].output.clone().unwrap_or_default();
    assert!(out.starts_with("the tests need a database") && out.contains("I tried three ways."), "{out:?}");

    // ISSUE: the step stays open and the person is asked; silence does not end it
    let mut e = engine();
    chained_agent(&mut e, "Both libraries would work.\nSTATUS: ISSUE should it use the v1 or the v2 API?");
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(!e.rt["a"].finished);
    assert_eq!(e.rt["a"].issue.as_deref(), Some("should it use the v1 or the v2 API?"));
    assert_eq!(e.rt["a"].status, Status::Attention);
    e.eval("a", t0 + Duration::from_secs(600));
    assert!(!e.rt["a"].finished);
    assert!(e.rt["b"].inputs.is_empty() && e.rt["b"].queue.is_empty(), "nothing moved on");
}

#[test]
fn an_agent_that_says_it_is_waiting_is_held_and_woken_not_finished() {
    let mut e = engine();
    chained_agent(&mut e, "The build is running.\nSTATUS: WAITING for the build to finish");
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(!e.rt["a"].finished, "waiting is not done");
    let w = e.rt["a"].waiting.as_ref().expect("weft noted what it waits for");
    assert_eq!((w.what.as_str(), w.checks, w.holding), ("for the build to finish", 0, true));
    // a long quiet is expected now: no second look for a while
    e.eval("a", t0 + Duration::from_secs(30));
    assert_eq!(e.rt["a"].waiting.as_ref().unwrap().checks, 0);
    // then it is woken (the text is typed into the session; here only the bookkeeping is visible)
    e.eval("a", t0 + Duration::from_secs(70));
    let w = e.rt["a"].waiting.as_ref().unwrap();
    assert_eq!((w.checks, w.holding), (1, false));
    assert!(e.rt["a"].cur.as_ref().unwrap().extra.contains("You said you were waiting for: for the build to finish"));
    // it checked, the build is done: the step ends with the real result
    says(&mut e, "The build finished: 412 tests passed.\nSTATUS: DONE");
    e.eval("a", t0 + Duration::from_secs(72));
    e.eval("a", t0 + Duration::from_secs(80));
    assert!(e.rt["a"].finished && e.rt["a"].waiting.is_none());
    let out = e.rt["a"].output.clone().unwrap_or_default();
    assert!(out.contains("412 tests passed") && !out.contains("STATUS"), "{out:?}");
    assert!(e.rt["b"].queue.iter().any(|q| q.text.contains("412 tests passed")));
}

#[test]
fn waiting_does_not_go_on_for_ever() {
    let mut e = engine();
    e.cfg.background_s = 180.0;
    e.cfg.recheck_s = 60.0;
    chained_agent(&mut e, "Queued the deploy.\nSTATUS: WAITING for the deploy");
    let mut t = Instant::now();
    e.eval("a", t);
    for _ in 0..40 {
        t += Duration::from_secs(70);
        e.eval("a", t);
        // each time it is woken it says the same thing and goes quiet again
        if e.rt["a"].issue.is_some() {
            break;
        }
        says(&mut e, "Still deploying.\nSTATUS: WAITING for the deploy");
        t += Duration::from_secs(5);
        e.eval("a", t);
    }
    assert!(e.rt["a"].issue.as_deref().unwrap_or("").starts_with("still waiting for for the deploy"), "{:?}", e.rt["a"].issue);
    assert!(!e.rt["a"].finished);
}

#[test]
fn a_chained_step_without_the_status_line_is_asked_once_and_the_answer_counts() {
    let mut e = engine();
    chained_agent(&mut e, "The build is running. I will get back to you when it is done.");
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(!e.rt["a"].finished, "a promise is not a result");
    assert!(e.rt["a"].cur.as_ref().unwrap().asked && e.rt["a"].cur.as_ref().unwrap().extra.contains("no STATUS line"));
    // asked, it did the work and answered; a bare word is enough now
    says(&mut e, "The build finished: all green.\nDONE");
    e.eval("a", t0 + Duration::from_secs(6));
    e.eval("a", t0 + Duration::from_secs(12));
    assert!(e.rt["a"].finished);
    let out = e.rt["a"].output.clone().unwrap_or_default();
    assert!(out.contains("all green"), "the result has what it found: {out:?}");
    // and if it still says nothing useful, silence is the end: a flow never hangs on it
    let mut e = engine();
    chained_agent(&mut e, "Hmm.");
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(!e.rt["a"].finished);
    e.eval("a", t0 + Duration::from_secs(20));
    assert!(e.rt["a"].finished);
}

#[test]
fn nothing_is_asked_when_nothing_takes_the_result() {
    let t0 = Instant::now();
    // a person chatting with an agent: no rule was given, so none is read
    let mut e = engine();
    deferring_agent(&mut e, Step::new("go".into(), vec![]));
    add(&mut e, "b", "next", "{{inputs}}");
    link(&mut e, "e1", "a", "b", On::Done);
    e.rt.get_mut("a").unwrap().cause = Cause::User;
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(e.rt["a"].finished, "user-driven: silence ends it as before");
}

#[test]
fn an_issue_keeps_the_step_open_until_the_person_answers() {
    let mut e = engine();
    deferring_agent(&mut e, with_command());
    let r = e.cmd_done("a".into(), true, "Which API version should it target?".into(), true);
    assert_eq!(r["ok"], true);
    assert_eq!(e.rt["a"].issue.as_deref(), Some("Which API version should it target?"));
    assert_eq!(e.rt["a"].status, Status::Attention);
    let t0 = Instant::now();
    e.eval("a", t0);
    // minutes of silence are the person's turn, not the end of the step and not a reason to remind the agent
    e.eval("a", t0 + Duration::from_secs(300));
    assert!(!e.rt["a"].finished && !e.rt["a"].cur.as_ref().unwrap().asked);
    assert_eq!(e.rt["a"].status, Status::Attention);
    // it survives a restart of the daemon
    let mut again = Rt::new(80, 24);
    again.restore(e.rt["a"].to_saved());
    assert_eq!(again.issue.as_deref(), Some("Which API version should it target?"));
    assert_eq!(again.status, Status::Attention);
    // and the agent can still finish later
    e.cmd_done("a".into(), true, "targeted v2".into(), false);
    assert!(e.rt["a"].finished && e.rt["a"].issue.is_none(), "reporting DONE closes the issue");
}

#[test]
fn a_window_that_comes_up_for_a_start_that_was_replaced_is_not_adopted() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    let came_up = || Ok(Spawned { pane: "%9".into(), win: "@9".into(), worktree: None });
    // the node was deleted while its window was being made, and a new node got the same id and started
    e.rt.get_mut("n1").unwrap().spawn = Some(7);
    e.rt.get_mut("n1").unwrap().spawning = true;
    e.on_spawned("n1".into(), 6, came_up());
    assert!(!e.rt["n1"].alive && e.rt["n1"].spawn == Some(7), "the old start's window is nobody's");
    assert!(e.panes.is_empty() && e.wins.is_empty());
    // its own start is taken
    e.on_spawned("n1".into(), 7, came_up());
    assert!(e.rt["n1"].alive && e.rt["n1"].spawn.is_none());
    assert_eq!(e.panes.get("%9").map(String::as_str), Some("n1"));
    // and a node that is stopped while starting does not come alive when the window arrives late
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    e.rt.get_mut("n1").unwrap().spawn = Some(3);
    e.rt.get_mut("n1").unwrap().spawning = true;
    e.kill("n1");
    e.on_spawned("n1".into(), 3, came_up());
    assert!(!e.rt["n1"].alive, "stopped means stopped");
}

#[test]
fn a_link_from_an_agent_that_is_answering_a_queued_prompt_hands_over_its_own_result() {
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add_kind(&mut e, "g", "approve", "gate", "");
    {
        // it finished its own run, and is now busy with a prompt queued on it by a step node
        let r = e.rt.get_mut("n1").unwrap();
        r.finished = false;
        r.armed = true;
        r.ok = Some(true);
        r.output = Some("the plan".into());
        r.reply_to = Some("s".into());
    }
    e.add_edge("n1", "g", On::Done).unwrap();
    assert_eq!(e.rt["g"].gate.held.as_deref().map(|h| h.contains("the plan")), Some(true), "the gate holds its result");
    // an agent in the middle of its own first run has no result yet: nothing is handed over
    let mut e = engine();
    add(&mut e, "n1", "a", "go");
    add_kind(&mut e, "g", "approve", "gate", "");
    e.rt.get_mut("n1").unwrap().armed = true;
    e.add_edge("n1", "g", On::Done).unwrap();
    assert!(e.rt["g"].gate.held.is_none());
}

#[test]
fn an_agent_that_has_not_been_given_its_task_is_told_with_it_and_not_before() {
    let mut e = engine();
    // up, its task still queued: typing the order now would race the task into a program that is starting
    alive_agent(&mut e, "a", "build ui");
    let r = e.cmd(None, Cmd::CheckAdd { node: "a".into(), kind: CheckKind::Eta, subject: String::new(), cmd: String::new(), every_s: 0 });
    assert_eq!(r["ok"], true, "{r}");
    assert!(e.rt["a"].queue.is_empty() && e.rt["a"].typing.is_none(), "nothing was typed or queued for it");
    assert!(e.take_orders("a").contains("eta <seconds>"), "the order is still to be told: it goes with the first prompt");
}

fn screen_with(text: &str) -> vt100::Parser {
    let mut p = vt100::Parser::new(14, 70, 0);
    p.process(text.replace('\n', "\r\n").as_bytes());
    p
}

#[test]
fn a_pasted_prompt_is_checked_to_have_been_taken() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let msg = "check which page could we use as a blog page.\nreason about that.";
    let still_in_input = screen_with("--------------------\n\u{276f} check which page could we use as a blog page.\n  reason about that.");
    let sent = screen_with("\u{276f} check which page could we use as a blog page.\n  reason about that.\n* Thinking\n                 xhigh\n--------------------\n\u{276f} ");
    let folded = screen_with("\u{276f} [Pasted text #1 +2 lines]\n* Thinking\n--------------------\n\u{276f} ");
    // a program that is still starting: it repaints, and the message is nowhere
    let starting = screen_with("Claude Code v2.1\n\nloading...");
    let empty = screen_with("--------------------\n\u{276f} Try \"edit <filepath> to...\"");
    let submit = ms(300);
    let mk = |checked, before: &vt100::Parser| Typing::new(msg, checked, before.screen(), t0);
    let entered = |mut t: Typing, at: Instant| {
        t.entered = true;
        t.at = at;
        t
    };

    // the Enter follows the paste after a moment
    assert_eq!(mk(true, &empty).next(still_in_input.screen(), t0, t0 + ms(100), submit), Typed::Wait);
    assert_eq!(mk(true, &empty).next(still_in_input.screen(), t0, t0 + ms(350), submit), Typed::Enter);
    // a steering note to a working agent is not checked
    assert_eq!(entered(mk(false, &empty), t0).next(empty.screen(), t0, t0 + ms(400), submit), Typed::Taken);

    // given its time, then: the text is still in the box, so the Enter was not taken as a submit
    let t = entered(mk(true, &empty), t0);
    assert_eq!(t.next(still_in_input.screen(), t0, t0 + ms(900), submit), Typed::Wait);
    assert_eq!(t.next(still_in_input.screen(), t0, t0 + ms(2000), submit), Typed::Enter);
    let mut t2 = entered(mk(true, &empty), t0);
    t2.enters = 2;
    assert_eq!(t2.next(still_in_input.screen(), t0, t0 + ms(2000), submit), Typed::GaveUp, "then the person is asked");
    // it left the box and shows in the transcript, or folded: taken
    assert_eq!(t.next(sent.screen(), t0 + ms(500), t0 + ms(2000), submit), Typed::Taken);
    assert_eq!(t.next(folded.screen(), t0 + ms(500), t0 + ms(2000), submit), Typed::Taken);
    // the same words already on the screen from an earlier turn are not a sign that this one was taken
    let again = entered(mk(true, &sent), t0);
    assert_ne!(again.next(sent.screen(), t0 - ms(5000), t0 + ms(4000), submit), Typed::Taken);

    // no trace of it: a screen that is still repainting is waited for ...
    let lost = entered(mk(true, &empty), t0);
    assert_eq!(lost.next(starting.screen(), t0 + ms(2900), t0 + ms(3200), submit), Typed::Wait);
    // ... one that has been still is waiting for the message: pasted again, once
    assert_eq!(lost.next(empty.screen(), t0 + ms(100), t0 + ms(1600), submit), Typed::Wait, "not before it had its time");
    assert_eq!(lost.next(empty.screen(), t0 + ms(100), t0 + ms(3200), submit), Typed::Paste);
    let mut lost2 = entered(mk(true, &empty), t0);
    lost2.pastes = 1;
    assert_eq!(lost2.next(empty.screen(), t0 + ms(100), t0 + ms(3200), submit), Typed::GaveUp);

    // the agent shows it started a turn after the Enter: taken, even with no echo anywhere
    assert!(lost.started_turn(true) && !lost.started_turn(false));
    assert!(!mk(true, &empty).started_turn(true), "not before the Enter");
    // one that was already in a turn when the message went out proves nothing by it
    assert!(!Typing { turn: true, ..entered(mk(true, &empty), t0) }.started_turn(true));
}

#[test]
fn a_report_from_the_agent_ends_the_check_on_its_message() {
    let mut e = engine();
    deferring_agent(&mut e, with_command());
    {
        // weft has not seen the request taken yet (its echo scrolled away), but the agent did the work
        let r = e.rt.get_mut("a").unwrap();
        r.typing = Some(Typing { entered: true, ..Typing::new("[weft] Request: Commit the work you just did.", true, r.parser.screen(), Instant::now()) });
    }
    e.cmd_done("a".into(), true, "00cacc8 Ignore build output".into(), false);
    assert!(e.rt["a"].typing.is_none(), "it is not pasted again, nor reported as not taken");
    assert!(e.rt["a"].finished && e.rt["a"].issue.is_none());
}

#[test]
fn an_agent_in_the_middle_of_a_turn_is_not_finished_by_a_pause() {
    let mut e = engine();
    alive_agent(&mut e, "a", "fix");
    e.flow.nodes[0].agent = "claude".into();
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.status = Status::Busy;
        r.cur = Some(Step::new("go".into(), vec![]));
        // a long think on a loaded machine: the spinner stops moving for a while
        r.parser.process("\u{25cf} Plan is set.\r\n\r\n\u{273b} Scampering\u{2026} (7m 28s \u{b7} \u{2193} 31.6k tokens)\r\n\r\n\u{276f} \r\n  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} esc to interrupt \u{b7} \u{2190} for agents\r\n".as_bytes());
        r.dirty = true;
    }
    let t0 = Instant::now();
    e.eval("a", t0);
    assert!(e.rt["a"].turn);
    e.eval("a", t0 + Duration::from_secs(30));
    assert!(!e.rt["a"].finished, "it is still in its turn");
    assert_eq!(e.rt["a"].status, Status::Busy);
    // the turn ends: the marker goes, the screen goes quiet, now it is done
    {
        let r = e.rt.get_mut("a").unwrap();
        r.parser.process("\x1b[2J\x1b[H\u{25cf} All fixed.\r\n\r\n\u{273b} Worked for 8m\r\n\r\n\u{276f} \r\n  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\r\n".as_bytes());
        r.dirty = true;
    }
    e.eval("a", t0 + Duration::from_secs(31));
    assert!(!e.rt["a"].turn);
    e.eval("a", t0 + Duration::from_secs(40));
    assert!(e.rt["a"].finished);
    assert!(e.rt["a"].output.as_deref().unwrap_or("").contains("All fixed"), "{:?}", e.rt["a"].output);
    // after its result it takes something up on its own: the card says so, and is done again after
    {
        let r = e.rt.get_mut("a").unwrap();
        r.parser.process("\x1b[2J\x1b[H\u{25cf} Background command finished.\r\n\r\n\u{276f} \r\n  bypass permissions on \u{b7} esc to interrupt\r\n".as_bytes());
        r.dirty = true;
    }
    e.eval("a", t0 + Duration::from_secs(41));
    assert_eq!(e.rt["a"].status, Status::Busy);
    {
        let r = e.rt.get_mut("a").unwrap();
        r.parser.process("\x1b[2J\x1b[H\u{25cf} Nothing else to do.\r\n\r\n\u{276f} \r\n  bypass permissions on\r\n".as_bytes());
        r.dirty = true;
    }
    e.eval("a", t0 + Duration::from_secs(42));
    assert_eq!(e.rt["a"].status, Status::Done);
    assert!(e.rt["a"].finished && e.rt["a"].output.as_deref().unwrap_or("").contains("All fixed"), "its result stands");
    // what an agent writes about the marker, above its status bar, holds nothing
    let mut r = Rt::new(80, 24);
    r.parser.process("The hint esc to interrupt shows while it works.\r\nline\r\nline\r\nline\r\nline\r\n\u{276f} \r\n  bypass permissions on\r\n".as_bytes());
    r.read_screen(e.cfg.agents.get("claude"), t0);
    assert!(!r.turn);
}

#[test]
fn a_prompt_not_yet_seen_taken_is_not_a_quiet_agent() {
    let mut r = Rt::new(80, 24);
    r.armed = true;
    let (quiet_d, ready_d, still) = (Duration::from_millis(2500), Duration::from_millis(1200), Duration::from_secs(5));
    assert_eq!(r.implied_status(Finish::Idle, still, quiet_d, ready_d), Status::Idle);
    // weft pasted a message and is still checking that the program took it: the card stays busy
    r.typing = Some(Typing::new("a message that is long enough to look for", true, r.parser.screen(), Instant::now()));
    assert_eq!(r.implied_status(Finish::Idle, still, quiet_d, ready_d), Status::Busy);
}

#[test]
fn a_queued_prompt_rewired_to_another_agent_is_asked_of_that_agent() {
    let mut e = engine();
    alive_agent(&mut e, "a", "first");
    alive_agent(&mut e, "b", "second");
    add_kind(&mut e, "g", "screenshot", "shot", "");
    link(&mut e, "e1", "b", "g", On::Done);
    let mut st = node("s", "prompt 2", "create a git repo");
    st.agent = "step".into();
    st.host = "a".into();
    e.flow.nodes.push(st);
    e.rt.insert("s".into(), Rt::new(80, 24));
    link(&mut e, "e2", "a", "s", On::Done);
    let host = |e: &Engine| e.node("s").unwrap().host.clone();
    // unlinked from a, it keeps the agent it had: nothing else is behind it
    e.cmd_edge_del("e2".into());
    assert_eq!(host(&e), "a");
    // linked after b's screenshot, it continues b's session (through the gate)
    e.add_edge("g", "s", On::Done).unwrap();
    assert_eq!(host(&e), "b");
    assert_eq!(e.resolve_host("s"), "b");
    // linked after a again, the link drawn last wins; removing it hands the prompt back to b
    e.add_edge("a", "s", On::Done).unwrap();
    assert_eq!(host(&e), "a");
    let eid = e.flow.edges.iter().find(|x| x.from == "a" && x.to == "s").unwrap().id.clone();
    e.cmd_edge_del(eid);
    assert_eq!(host(&e), "b");
    // a git step or a note is no session to go on with: the agent stays
    add_kind(&mut e, "c", "commit", "git", "");
    e.add_edge("c", "s", On::Done).unwrap();
    assert_eq!(host(&e), "b");
}

#[test]
fn needs_you_after_a_result_ends_when_the_person_has_answered() {
    let mut e = engine();
    alive_agent(&mut e, "a", "build");
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.finished = true;
        r.ok = Some(true);
        r.status = Status::Done;
    }
    let t0 = Instant::now();
    // a question that comes after the result
    e.cmd_done("a".into(), true, "Should I install it too?".into(), true);
    e.eval("a", t0);
    assert_eq!(e.rt["a"].status, Status::Attention);
    // answered in its terminal (an Enter clears the question): the card is done again, not stuck
    e.rt.get_mut("a").unwrap().issue = None;
    e.eval("a", t0 + Duration::from_secs(1));
    assert_eq!(e.rt["a"].status, Status::Done);
    // a dialog on the screen after the result asks the person while it is up, and only then
    e.rt.get_mut("a").unwrap().attn = true;
    e.eval("a", t0 + Duration::from_secs(2));
    assert_eq!(e.rt["a"].status, Status::Attention);
    e.rt.get_mut("a").unwrap().attn = false;
    e.eval("a", t0 + Duration::from_secs(3));
    assert_eq!(e.rt["a"].status, Status::Done);
}

#[test]
fn a_message_to_an_agent_at_a_dialog_waits_until_the_dialog_is_closed() {
    let mut e = engine();
    deferring_agent(&mut e, with_command());
    // a dialog takes a pasted message as its answer (that is how an "update now" gets confirmed)
    let t0 = Instant::now();
    e.eval("a", t0);
    e.rt.get_mut("a").unwrap().attn = true;
    e.eval("a", t0);
    assert_eq!(e.rt["a"].status, Status::Attention);
    e.cmd_send("a".into(), "and add tests".into(), SendMode::Auto, true);
    e.cmd_send("a".into(), "now".into(), SendMode::Now, true);
    assert_eq!(e.rt["a"].queue.len(), 2, "both wait in the queue");
}

/// `id` finished `ago_min` minutes ago, with `ok` and a result.
fn finished(e: &mut Engine, id: &str, ok: bool, ago_min: u64) {
    let rt = e.rt.get_mut(id).unwrap();
    rt.finished = true;
    rt.armed = true;
    rt.ok = Some(ok);
    rt.output = Some(format!("result of {id}"));
    rt.status = if ok { Status::Done } else { Status::Failed };
    rt.since = now_ms() - ago_min * 60_000;
}

fn due(e: &Engine) -> Vec<Vec<String>> {
    let plan = e.filing_plan(&e.blocked_map());
    let mut due: Vec<Vec<String>> = plan
        .into_iter()
        .filter(|(_, at)| *at <= now_ms())
        .map(|(mut f, _)| {
            f.sort();
            f
        })
        .collect();
    due.sort();
    due
}

#[test]
fn a_lone_finished_card_files_itself_after_half_an_hour_and_comes_back_as_it_was() {
    let mut e = engine();
    add(&mut e, "n1", "fix login", "fix the login");
    add(&mut e, "n2", "still busy", "go");
    finished(&mut e, "n1", true, 10);
    // done ten minutes ago: it goes in twenty, and the card can say so
    let at = e.filing_map(&e.blocked_map())["n1"];
    assert!(at.abs_diff(e.rt["n1"].since + 1_800_000) < 1000);
    assert!(!e.filing_map(&e.blocked_map()).contains_key("n2"), "a card that never ran is not finished");
    e.tick_archive();
    assert!(e.node("n1").is_some(), "not before its time");
    // half an hour on: filed, process and all, with what it knew
    finished(&mut e, "n1", true, 31);
    e.archive_look = 0;
    e.tick_archive();
    assert!(e.node("n1").is_none() && !e.rt.contains_key("n1"));
    assert!(e.node("n2").is_some());
    let g = &e.archive.groups[0];
    assert!(g.auto && g.nodes[0].name == "fix login");
    assert_eq!(g.state["n1"].output.as_deref(), Some("result of n1"));
    let gid = g.id.clone();
    assert!(e.archive_path.exists(), "the archive is on disk");
    // filed work keeps its id: a new card does not take it
    let r = e.cmd_node_add(json!({"agent": "claude", "prompt": "next"}), None, false);
    assert_eq!(r["id"], "n3");
    // back on the canvas where it was asked to go, with its result, and a new half hour
    let r = e.cmd_revive(gid, 40.0, 0.0);
    assert_eq!(r["ids"], json!(["n1"]));
    assert!(e.archive.groups.is_empty());
    assert_eq!(e.node("n1").unwrap().x, 40.0);
    assert_eq!(e.rt["n1"].output.as_deref(), Some("result of n1"));
    assert_eq!(e.eff_status("n1", &HashMap::new()), Status::Done);
    assert!(!e.rt["n1"].alive);
    assert!(e.filing_map(&e.blocked_map())["n1"] >= now_ms() + 1_799_000, "its clock starts again");
    // a restart keeps "done 31 min ago" (and the clock with it)
    let mut back = Rt::new(80, 24);
    back.restore(e.rt["n1"].to_saved());
    assert_eq!(back.since, e.rt["n1"].since);
    assert_eq!(back.revived, e.rt["n1"].revived);
}

#[test]
fn a_flow_is_filed_as_one_an_hour_after_its_last_step_unless_something_holds_it() {
    let mut e = engine();
    add(&mut e, "n1", "plan", "plan it");
    add(&mut e, "n2", "build", "build it");
    add(&mut e, "n3", "review", "review it");
    add(&mut e, "n6", "elsewhere", "other work");
    add_kind(&mut e, "n4", "spec", "note", "the spec");
    add_kind(&mut e, "n5", "style", "note", "house style");
    add(&mut e, "n7", "on failure", "repair it");
    link(&mut e, "e1", "n1", "n2", On::Done);
    link(&mut e, "e2", "n2", "n3", On::Done);
    link(&mut e, "e3", "n4", "n2", On::Done);
    link(&mut e, "e4", "n5", "n2", On::Done);
    link(&mut e, "e5", "n5", "n6", On::Done);
    // a branch for a failure that did not happen: it never runs, and it does not hold the flow
    link(&mut e, "e6", "n3", "n7", On::Fail);
    // a note goes with the one flow it feeds; one that feeds two flows stays
    let mut flows = e.flows();
    flows.iter_mut().for_each(|f| f.sort());
    assert!(flows.contains(&vec!["n1".into(), "n2".into(), "n3".into(), "n4".into(), "n7".into()]));
    assert!(flows.contains(&vec!["n6".to_string()]));
    assert!(!flows.iter().flatten().any(|i| i == "n5"));
    finished(&mut e, "n1", true, 150);
    finished(&mut e, "n2", true, 120);
    assert_eq!(due(&e), Vec::<Vec<String>>::new(), "the review has not run yet: the flow is not finished");
    finished(&mut e, "n3", true, 30);
    assert_eq!(due(&e), Vec::<Vec<String>>::new(), "an hour after the LAST step, not the first");
    finished(&mut e, "n3", true, 61);
    assert_eq!(due(&e), vec![vec!["n1", "n2", "n3", "n4", "n7"]]);
    // a kept card keeps its flow, a failure holds it for a person, a card at work holds it
    e.flow.nodes.iter_mut().find(|n| n.id == "n2").unwrap().keep = true;
    assert!(due(&e).is_empty());
    e.flow.nodes.iter_mut().find(|n| n.id == "n2").unwrap().keep = false;
    finished(&mut e, "n2", false, 120);
    assert!(due(&e).is_empty());
    finished(&mut e, "n2", true, 120);
    e.rt.get_mut("n2").unwrap().queue.push_back(QItem { id: 1, text: "more".into(), shown: "more".into(), src: Src::You });
    assert!(due(&e).is_empty());
    e.rt.get_mut("n2").unwrap().queue.clear();
    // filed as one drop: its links go with it, the shared note stays with what it still feeds
    e.tick_archive();
    assert_eq!(e.archive.groups.len(), 1);
    assert_eq!(e.archive.groups[0].nodes.len(), 5);
    let ids: Vec<&str> = e.flow.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["n6", "n5"]);
    assert_eq!(e.flow.edges.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["e5"]);
    // back again: every link returns, the one from the shared note too, and nothing is delivered
    let gid = e.archive.groups[0].id.clone();
    e.cmd_revive(gid, 0.0, 0.0);
    let mut eids: Vec<&str> = e.flow.edges.iter().map(|x| x.id.as_str()).collect();
    eids.sort();
    assert_eq!(eids, vec!["e1", "e2", "e3", "e4", "e5", "e6"]);
    assert!(e.rt.values().all(|r| r.queue.is_empty()));
    assert_eq!(e.rt["n4"].output.as_deref(), Some("the spec"));
}

#[test]
fn what_is_still_at_work_stays_on_the_canvas_when_dropped_on_the_archive() {
    let mut e = engine();
    add(&mut e, "n1", "busy one", "go");
    add(&mut e, "n2", "done one", "go");
    add_kind(&mut e, "n3", "follow-up", "step", "and the tests?");
    e.flow.nodes.iter_mut().find(|n| n.id == "n3").unwrap().host = "n2".into();
    link(&mut e, "e1", "n2", "n3", On::Done);
    e.rt.get_mut("n1").unwrap().cur = Some(Step::new("go".into(), Vec::new()));
    finished(&mut e, "n2", true, 1);
    let r = e.cmd_archive(vec!["n1".into()]);
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("busy one is still at work"));
    // the finished agent goes, with the prompt queued on it; the busy one stays
    let r = e.cmd_archive(vec!["n1".into(), "n2".into()]);
    assert_eq!(r["ok"], true);
    assert_eq!(r["busy"], json!(["busy one"]));
    assert!(e.node("n1").is_some() && e.node("n2").is_none() && e.node("n3").is_none());
    let g = &e.archive.groups[0];
    assert!(!g.auto);
    assert_eq!(g.edges.len(), 1);
    // and it can be forgotten for good
    let gid = g.id.clone();
    assert_eq!(e.cmd_archive_get(gid.clone())["nodes"][0]["out"], "result of n2");
    e.cmd_archive_del(gid);
    assert!(e.archive.groups.is_empty());
    assert_eq!(e.cmd_node_add(json!({"agent": "claude"}), None, false)["id"], "n2", "a forgotten id is free again");
}

#[test]
fn the_archive_file_stays_out_of_git() {
    let dir = std::env::temp_dir().join(format!("weft-gitignore-{}", std::process::id()));
    let p = dir.join(".weft").join(".gitignore");
    let _ = std::fs::remove_dir_all(&dir);
    ensure_weft_gitignore(&dir);
    assert!(std::fs::read_to_string(&p).unwrap().lines().any(|l| l == "archive.json"));
    // a folder from before the archive gets the line added, and what was there stays
    write_atomic(&p, b"run/\nshots/\nstate.json").unwrap();
    ensure_weft_gitignore(&dir);
    ensure_weft_gitignore(&dir);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "run/\nshots/\nstate.json\narchive.json\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_step_an_agent_answers_survives_a_restart() {
    let mut r = Rt::new(80, 24);
    r.cur = Some(Step { rule: true, asked: true, ..Step::new("recheck it".into(), vec!["x".into()]) });
    r.reply_to = Some("s".into());
    let saved = serde_json::to_string(&r.to_saved()).unwrap();
    let mut back = Rt::new(80, 24);
    back.restore(serde_json::from_str(&saved).unwrap());
    let c = back.cur.as_ref().expect("the step is kept");
    assert!(c.rule && c.asked && c.prompt == "recheck it");
    assert_eq!(back.reply_to.as_deref(), Some("s"));
    // the state file of an older build has none of it, and still loads
    let old: Saved = serde_json::from_str(r#"{"armed": true}"#).unwrap();
    assert!(old.cur.is_none() && old.reply_to.is_none());
}

#[test]
fn after_a_restart_from_an_older_build_the_answer_still_goes_to_its_step() {
    let mut e = engine();
    alive_agent(&mut e, "a", "So I am working");
    let mut st = node("s", "prompt", "recheck this is working. increase the font size by 20%.");
    st.agent = "step".into();
    st.host = "a".into();
    e.flow.nodes.push(st);
    e.rt.insert("s".into(), Rt::new(80, 24));
    link(&mut e, "e1", "a", "s", On::Done);
    add(&mut e, "b", "prompt 3", "{{inputs}}");
    link(&mut e, "e2", "s", "b", On::Ok);
    {
        // what an older daemon left: the agent mid-run, the step busy, no record of who answers what
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.runs = 4;
        r.asked = "recheck this is working. increase the font size by 20%.".into();
        let echo = format!("\u{25cf} Committed cd82c97.\r\n\u{276f} recheck this is working. increase the font size by 20%.\r\n  {}\r\n", words::status_rule(false));
        r.parser.process(format!("{echo}\u{25cf} Rechecked: the font is 20% bigger.\r\n").as_bytes());
        e.rt.get_mut("s").unwrap().runs = 1;
    }
    e.resume_steps();
    assert_eq!(e.rt["a"].reply_to.as_deref(), Some("s"));
    assert_eq!(e.rt["s"].step_wait.as_deref(), Some("a"));
    assert!(e.rt["a"].cur.as_ref().is_some_and(|c| c.rule), "a step of a chain: its STATUS line is read");
    // it finishes its answer
    says(&mut e, "  STATUS: DONE");
    let t0 = Instant::now();
    e.eval("a", t0);
    e.eval("a", t0 + Duration::from_secs(5));
    assert!(e.rt["s"].finished, "the step got its answer");
    let out = e.rt["s"].output.clone().unwrap_or_default();
    assert!(out.contains("20% bigger") && !out.contains("cd82c97") && !out.contains("STATUS"), "{out:?}");
    assert_eq!(e.rt["s"].runs, 1, "not started all over again");
    assert!(e.rt["a"].queue.is_empty());
    assert!(e.rt["b"].queue.iter().any(|q| q.text.contains("20% bigger")), "the chain goes on");
    // an agent that did not come back answers nothing
    let mut e = engine();
    add(&mut e, "a", "gone", "go");
    e.rt.get_mut("a").unwrap().cur = Some(Step::new("go".into(), vec![]));
    e.rt.get_mut("a").unwrap().reply_to = Some("s".into());
    e.resume_steps();
    assert!(e.rt["a"].cur.is_none() && e.rt["a"].reply_to.is_none());
}

#[test]
fn a_resized_card_keeps_its_size_and_shows_more_of_its_terminal() {
    let mut e = engine();
    let r = e.cmd(None, Cmd::NodeAdd { node: json!({"agent": "claude", "draft": true}), from: None, run: false });
    let id = r["id"].as_str().unwrap().to_string();
    let saved = |e: &Engine| serde_json::to_value(e.node(&id).unwrap()).unwrap();
    assert!(saved(&e).get("w").is_none() && saved(&e).get("h").is_none(), "the default size is not written down");
    let r = e.cmd(None, Cmd::NodeSet { id: id.clone(), patch: json!({"w": 640, "h": 480}) });
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!((e.node(&id).unwrap().w, e.node(&id).unwrap().h), (640.0, 480.0));
    e.cmd(None, Cmd::NodeSet { id: id.clone(), patch: json!({"prompt": "go"}) });
    assert_eq!(e.node(&id).unwrap().w, 640.0, "another edit leaves the size alone");
    e.cmd(None, Cmd::NodeSet { id: id.clone(), patch: json!({"w": 0, "h": 0}) });
    assert!(saved(&e).get("w").is_none(), "back to the default");
    assert_eq!(crate::model::preview_rows(0.0), 12, "a card of the default height");
    assert_eq!(crate::model::preview_rows(232.0), 12);
    assert!(crate::model::preview_rows(480.0) > 25, "a taller card shows more of the terminal");
}

// ---- the chain audit: hosts follow the graph, a session is not sent its own answer, long answers
// ---- keep their head, a stale busy marker ends, and what tmux shows wins over weft's copy

fn mk_step(e: &mut Engine, id: &str, name: &str, host: &str) {
    let mut st = node(id, name, &format!("do {name}"));
    st.agent = "step".into();
    st.host = host.into();
    e.flow.nodes.push(st);
    e.rt.insert(id.into(), Rt::new(80, 24));
}

#[test]
fn the_steps_behind_a_step_that_moves_to_another_agent_go_with_it() {
    let mut e = engine();
    alive_agent(&mut e, "a", "first");
    alive_agent(&mut e, "b", "second");
    mk_step(&mut e, "s9", "nine", "a");
    add_kind(&mut e, "g", "screenshot", "shot", "");
    mk_step(&mut e, "s3", "three", "a");
    e.add_edge("s9", "g", On::Done).unwrap();
    e.add_edge("g", "s3", On::Done).unwrap();
    assert_eq!(e.node("s3").unwrap().host, "a");
    // s9 is drawn after b: it follows b, and the prompt behind its screenshot gate with it
    e.add_edge("b", "s9", On::Ok).unwrap();
    assert_eq!(e.node("s9").unwrap().host, "b");
    assert_eq!(e.node("s3").unwrap().host, "b");
    assert_eq!(e.resolve_host("s3"), "b");
    // a link into a step that keeps leading to its agent leaves it alone, and so does no link
    assert_eq!(e.reconcile_hosts(), 0);
    mk_step(&mut e, "lone", "lone", "a");
    assert_eq!(e.reconcile_hosts(), 0);
}

#[test]
fn a_board_with_a_stale_host_is_put_right() {
    let mut e = engine();
    alive_agent(&mut e, "a", "first");
    alive_agent(&mut e, "b", "second");
    mk_step(&mut e, "s1", "one", "b");
    mk_step(&mut e, "s2", "two", "a"); // stale: it hangs on s1, which is b's
    mk_step(&mut e, "s3", "three", "a"); // and so does this one, behind s2
    link(&mut e, "e1", "b", "s1", On::Done);
    link(&mut e, "e2", "s1", "s2", On::Ok);
    link(&mut e, "e3", "s2", "s3", On::Ok);
    assert_eq!(e.reconcile_hosts(), 2);
    assert_eq!(e.node("s2").unwrap().host, "b");
    assert_eq!(e.node("s3").unwrap().host, "b");
    assert_eq!(e.reconcile_hosts(), 0);
}

#[test]
fn a_step_is_not_sent_what_its_own_agent_answered_in_that_session() {
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    alive_agent(&mut e, "b", "other");
    mk_step(&mut e, "s1", "one", "a");
    mk_step(&mut e, "s2", "two", "a");
    link(&mut e, "e1", "a", "s1", On::Done);
    link(&mut e, "e2", "s1", "s2", On::Ok);
    e.settle("s1", true, "THE-ANSWER-OF-S1".into());
    let q: Vec<String> = e.rt["a"].queue.iter().map(|q| q.text.clone()).collect();
    assert_eq!(q.len(), 1);
    assert!(q[0].starts_with("do two"), "{q:?}");
    assert!(!q[0].contains("THE-ANSWER-OF-S1"), "its own answer is not sent back: {q:?}");
    // what another agent produced is news to it, and so is what a check found out
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    alive_agent(&mut e, "b", "other");
    mk_step(&mut e, "s2", "two", "a");
    let other = Input { from: "b".into(), name: "other".into(), ok: true, text: "NEWS-FROM-B".into(), kind: InputKind::Result };
    assert!(!e.in_session_of("b", "a"));
    assert!(e.in_session_of("a", "a"));
    let (body, _) = e.render_parts(e.node("s2").unwrap(), &[other]);
    assert!(body.contains("NEWS-FROM-B"));
}

/// Draw `rows` as the whole screen of `id` and let the step see it, like one frame of a program
/// that shows only the end of what it has said.
fn show(e: &mut Engine, id: &str, rows: &[String]) {
    let chrome = e.cfg.chrome.clone();
    let r = e.rt.get_mut(id).unwrap();
    r.parser.process(b"\x1b[2J\x1b[H");
    r.parser.process(rows.join("\r\n").as_bytes());
    r.accumulate(&chrome);
}

#[test]
fn the_head_of_an_answer_longer_than_the_window_reaches_the_next_step() {
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.painted = true;
        r.cur = Some(Step { rule: true, ..Step::new("do it".into(), vec!["an earlier line".into()]) });
    }
    let mut said: Vec<String> = (1..=14).map(|i| format!("answer line number {i}")).collect();
    said.push("STATUS: DONE".into());
    // a window of six rows following the end of it
    for end in 6..=said.len() {
        show(&mut e, "a", &said[end - 6..end]);
    }
    e.step_done("a", true, None, How::Quiet);
    let out = e.rt["a"].output.clone().unwrap();
    assert!(out.starts_with("answer line number 1\n"), "the head is there: {out:?}");
    assert!(out.ends_with("answer line number 14"), "and the status line is not: {out:?}");
    assert_eq!(out.lines().count(), 14);
}

#[test]
fn a_step_whose_result_nobody_takes_is_read_from_the_last_screen_only() {
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.painted = true;
        r.cur = Some(Step::new("do it".into(), Vec::new())); // not a link of a chain: no rule
    }
    let said: Vec<String> = (1..=10).map(|i| format!("answer line number {i}")).collect();
    for end in 6..=said.len() {
        show(&mut e, "a", &said[end - 6..end]);
    }
    assert!(e.rt["a"].cur.as_ref().unwrap().acc.is_empty(), "nothing is collected for it");
    e.step_done("a", true, None, How::Quiet);
    assert_eq!(e.rt["a"].output.clone().unwrap().lines().count(), 6);
}

#[test]
fn a_turn_marker_on_a_screen_that_has_not_moved_ends_after_the_hold() {
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    let t0 = Instant::now();
    let hold = Duration::from_secs(600);
    let r = e.rt.get_mut("a").unwrap();
    r.armed = true;
    r.turn = true;
    r.screen_changed = t0;
    r.hold_for_background(true, hold, t0 + Duration::from_secs(599));
    assert!(r.turn, "within the hold it is a turn that pauses");
    r.hold_for_background(true, hold, t0 + Duration::from_secs(601));
    assert!(!r.turn, "after it, it is a leftover");
    // while the screen does move, the marker keeps holding however long the turn is
    r.turn = true;
    r.screen_changed = t0 + Duration::from_secs(3000);
    r.hold_for_background(true, hold, t0 + Duration::from_secs(3100));
    assert!(r.turn);
}

#[test]
fn what_tmux_shows_wins_over_a_copy_that_kept_a_stale_marker() {
    let mut e = engine();
    alive_agent(&mut e, "a", "agent");
    e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "claude".into();
    let cfg = e.cfg.agents.get("claude").cloned();
    let t0 = Instant::now();
    {
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.painted = true;
        r.parser.process("all done\r\n\r\n  ⏵⏵ bypass permissions on (shift+tab to cycle) · esc to interrupt".as_bytes());
        r.read_screen(cfg.as_ref(), t0);
        assert!(r.turn, "the copy says it is in a turn");
        assert!(!r.needs_check(t0 + Duration::from_secs(5)), "not yet: a turn redraws its clock");
        assert!(r.needs_check(t0 + Duration::from_secs(25)));
        r.verified = Some(t0);
        assert!(!r.needs_check(t0 + Duration::from_secs(60)), "asked once per stall");
    }
    // tmux shows the status row without the marker (and the whole pane one row each)
    let real = Snap {
        text: "all done\n\n  ⏵⏵ bypass permissions on (shift+tab to cycle)\n".into(),
        cursor: Some((0, 0)),
        alt: false,
        rows: 24,
        cols: 80,
    };
    e.on_snapshot("a".into(), Some(real));
    let r = &e.rt["a"];
    assert!(!r.turn);
    assert!(!r.parser.screen().contents().contains("esc to interrupt"));
    assert!(!r.needs_check(t0 + Duration::from_secs(99)));
    // a snapshot that is the same as the copy changes nothing
    let before = e.rt["a"].hash;
    let same = Snap {
        text: "all done\n\n  ⏵⏵ bypass permissions on (shift+tab to cycle)\n".into(),
        cursor: Some((0, 0)),
        alt: false,
        rows: 24,
        cols: 80,
    };
    e.on_snapshot("a".into(), Some(same));
    assert_eq!(e.rt["a"].hash, before);
}

#[test]
fn a_model_answer_becomes_a_short_card_name() {
    use super::titles::clean_title;
    assert_eq!(clean_title("Fix slugify umlauts\n").as_deref(), Some("Fix slugify umlauts"));
    assert_eq!(clean_title("\"Fix German Umlaut Slugification.\"").as_deref(), Some("Fix German Umlaut Slugification"));
    assert_eq!(clean_title("Sure!\n\nTitle: **Wrap text tests**").as_deref(), Some("Wrap text tests"));
    // whole words only when it runs long
    assert_eq!(clean_title("Rework the archive countdown chip and footer").as_deref(), Some("Rework the archive countdown chip and"));
    assert_eq!(clean_title("   \n"), None);
    assert_eq!(clean_title(&"word ".repeat(30)), None);
}

#[test]
fn git_steps_are_named_after_the_work_they_carry() {
    let mut e = engine();
    alive_agent(&mut e, "a", "Fix slugify");
    alive_agent(&mut e, "b", "Wrap text");
    alive_agent(&mut e, "d", "Docs pass");
    let mk = |e: &mut Engine, op: &str| match e.cmd_node_add(json!({"agent": "git", "git_op": op}), None, false)["id"].as_str() {
        Some(id) => id.to_string(),
        None => panic!("no node"),
    };
    let c1 = mk(&mut e, "commit");
    let c2 = mk(&mut e, "commit");
    let name = |e: &Engine, id: &str| e.node(id).unwrap().name.clone();
    // nothing linked yet: the plain verb, and no "commit 2"
    assert_eq!(name(&e, &c1), "commit");
    e.add_edge("a", &c1, On::Done).unwrap();
    e.add_edge("b", &c2, On::Done).unwrap();
    assert_eq!(name(&e, &c1), "commit: Fix slugify");
    assert_eq!(name(&e, &c2), "commit: Wrap text");
    // a push names what the commits before it carry, a PR what the push carries
    let p = mk(&mut e, "push");
    e.add_edge(&c1, &p, On::Done).unwrap();
    e.add_edge(&c2, &p, On::Done).unwrap();
    assert_eq!(name(&e, &p), "push: Fix slugify + Wrap text");
    let pr = mk(&mut e, "pr");
    e.add_edge(&p, &pr, On::Done).unwrap();
    assert_eq!(name(&e, &pr), "PR: Fix slugify + Wrap text");
    let c3 = mk(&mut e, "commit");
    e.add_edge("d", &c3, On::Done).unwrap();
    e.add_edge(&c3, &p, On::Done).unwrap();
    assert_eq!(name(&e, &p), "push: Fix slugify + 2 more");
    // the agent's name changes: so does its commit's
    e.cmd_node_set("a".into(), json!({"name": "Slug umlauts"}));
    assert_eq!(name(&e, &c1), "commit: Slug umlauts");
    // a name given by hand stays
    e.cmd_node_set(c2.clone(), json!({"name": "ship wrap"}));
    e.cmd_node_set("b".into(), json!({"name": "Wrap words"}));
    assert_eq!(name(&e, &c2), "ship wrap");
    // a queued prompt or a gate between agent and commit: the agent behind it
    alive_agent(&mut e, "x", "Lint");
    mk_step(&mut e, "s", "then test", "x");
    link(&mut e, "ex", "x", "s", On::Done);
    let c4 = mk(&mut e, "commit");
    e.add_edge("s", &c4, On::Done).unwrap();
    assert_eq!(name(&e, &c4), "commit: Lint");
}

#[test]
fn git_steps_from_older_builds_get_named_by_their_work() {
    use super::titles::is_plain_git_name;
    assert!(is_plain_git_name("commit", GitOp::Commit));
    assert!(is_plain_git_name("commit 2", GitOp::Commit));
    assert!(is_plain_git_name("open PR 3", GitOp::Pr));
    assert!(!is_plain_git_name("commit slug", GitOp::Commit));
    assert!(!is_plain_git_name("push", GitOp::Commit));
}

#[test]
fn a_late_name_does_not_replace_a_newer_prompt_or_your_own() {
    let mut e = engine();
    let id = e.cmd_node_add(json!({"agent": "claude", "prompt": "Fix the slugify function in textkit/slug.py"}), None, false)["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(e.node(&id).unwrap().name, "Fix the slugify function");
    e.on_title(&id, "Fix the slugify function in textkit/slug.py", Some("Fix slugify".into()));
    assert_eq!(e.node(&id).unwrap().name, "Fix slugify");
    // the prompt changed after the request went out: that answer is stale
    e.cmd_node_set(id.clone(), json!({"prompt": "Wrap long lines in textkit/wrap.py"}));
    e.on_title(&id, "Fix the slugify function in textkit/slug.py", Some("Old name".into()));
    assert_eq!(e.node(&id).unwrap().name, "Wrap long lines");
    // renamed by hand: no model renames it again
    e.cmd_node_set(id.clone(), json!({"name": "mine"}));
    e.on_title(&id, "Wrap long lines in textkit/wrap.py", Some("Wrap lines".into()));
    assert_eq!(e.node(&id).unwrap().name, "mine");
}
