// Every argument keeps its original pattern in the typed callback.
fn foo(
    a: i32,
    (b, c): (i32, i32),
    d: i32,
    Wrapper(e): Wrapper<i32>,
    [f, g]: [i32; 2],
    h: i32,
    Point { x, y }: Point,
) -> CheckResult {
    check((a, b, c, d, e, f, g, h, x, y))
}
