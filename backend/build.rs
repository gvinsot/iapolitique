//! Rebuild when migrations change: they are embedded by `sqlx::migrate!`.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
