use agentland_core::supervisor::{safe_to_type, turn_running};

const IDLE: &str = include_str!("fixtures/pane-idle.txt");
const BUSY: &str = include_str!("fixtures/pane-busy.txt");
const TYPING: &str = include_str!("fixtures/pane-typing.txt");

#[test]
fn a_real_idle_frame_is_safe_to_type_into() {
    assert!(!turn_running(IDLE), "nothing is running in this frame");
    assert!(
        safe_to_type(IDLE, IDLE),
        "the leader would never be woken:\n{IDLE}"
    );
}

#[test]
fn a_real_running_turn_is_left_alone() {
    assert!(turn_running(BUSY), "the spinner means a turn is in flight:\n{BUSY}");
    assert!(!safe_to_type(BUSY, BUSY));
}

#[test]
fn a_composer_with_text_in_it_is_never_clobbered() {
    assert!(
        !safe_to_type(TYPING, TYPING),
        "there is a sentence sitting in this composer:\n{TYPING}"
    );
}

#[test]
fn a_turn_that_has_just_ended_can_be_written_to() {
    assert!(!safe_to_type(IDLE, BUSY), "a turn started; leave it alone");
    assert!(
        safe_to_type(BUSY, IDLE),
        "the pane log grows forever, so demanding two identical reads never wakes anyone"
    );
}

const NOISY: &str = include_str!("fixtures/pane-idle-noisy.txt");

#[test]
fn footer_fragments_printed_after_the_prompt_do_not_hide_it() {
    assert!(!turn_running(NOISY), "the turn ended: {NOISY}");
    assert!(
        safe_to_type(NOISY, NOISY),
        "a real frame ends in redraw fragments, and reading only the last line \
         concludes there is no composer — which is how a leader is never woken:\n{NOISY}"
    );
}
