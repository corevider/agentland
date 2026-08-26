use agentland_core::{read_context, ContextReading};

const RECORDED: &str = include_str!("fixtures/claude-statusline.txt");

#[test]
fn the_parser_reads_the_output_a_real_agent_actually_produced() {
    let reading = read_context(RECORDED).expect("the recorded session reports its context");

    match reading {
        ContextReading::TokensUsed(tokens) => {
            assert_eq!(
                tokens, 44_500,
                "the last status line in the recording says 44.5k"
            );
        }
        ContextReading::PercentLeft(percent) => {
            panic!("this engine reports tokens, not {percent}%")
        }
    }
}

#[test]
fn a_session_that_never_mentions_context_reports_nothing() {
    let ordinary = "$ cargo test\n   Compiling agentland-core\ntest result: ok. 40 passed\n";
    assert_eq!(read_context(ordinary), None);
}
