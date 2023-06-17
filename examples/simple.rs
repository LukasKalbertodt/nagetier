

fn main() {
    let module = nagetier::include_wgsl!("foo.wgsl");
    dbg!(module.label);
}
