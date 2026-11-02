//! Embeds the skills under `skills/`, which the server serves at `skill://time/…`.
fn main() {
    veoveo_skills::build::embed_skills("skills").expect("Time's skills");
}
