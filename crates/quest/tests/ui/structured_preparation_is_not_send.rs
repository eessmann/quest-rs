use quest::PreparedStructuredProgram;
fn require_send<T: Send>() {}
fn main() {
    require_send::<PreparedStructuredProgram<'static>>();
}
