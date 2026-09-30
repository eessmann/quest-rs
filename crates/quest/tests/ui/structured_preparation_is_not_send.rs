use quest::PreparedProgram;
fn require_send<T: Send>() {}
fn main() {
    require_send::<PreparedProgram<'static>>();
}
