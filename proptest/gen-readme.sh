#! /bin/sh

# Generate `README.md` from the crate documentation, plus some extra stuff.

set -eu

cat readme-prologue.md >README.md
printf '\n' >>README.md
awk '
    # The book keeps hidden doctest setup lines in Rust code fences. The README
    # keeps those examples display-only, so strip hidden setup and retain the
    # historical non-doctest fence hints.
    function hidden() {
        return opener ~ /^```rust/ && $0 ~ /^[[:space:]]*#( |$)/;
    }

    FNR == 1 && NR != 1 {
        print "";
    }

    /NOREADME/ {
        next;
    }

    function print_fence(first,   readme_fence) {
        readme_fence = opener;
        fence_printed = 1;
        snip = 0;
        blank_after_use = 0;

        if (opener == "```rust,should_panic") {
            readme_fence = "```rust,ignore";
            blank_after_use = first == "// Bring the macros and other important things into scope.";
        } else if (opener == "```rust") {
            if (first ~ /^fn parse_date/) {
                readme_fence = "```rust,no_run";
            } else if (first == "proptest! {") {
                readme_fence = "```rust,ignore";
                snip = 1;
            } else if (first ~ /^[[:space:]]+println!/) {
                readme_fence = "```rust,ignore";
            }
        }

        print readme_fence;
    }

    /^```/ {
        if (in_code) {
            if (fence_printed) {
                print "```";
            }
            in_code = 0;
            opener = "";
            first_code_line = 0;
            fence_printed = 0;
            snip = 0;
            blank_after_use = 0;
            next;
        }

        in_code = 1;
        opener = $0;
        first_code_line = 1;
        fence_printed = 0;
        snip = 0;
        blank_after_use = 0;
        next;
    }

    in_code {
        if (hidden()) {
            next;
        }

        if (first_code_line && $0 ~ /^[[:space:]]*$/) {
            next;
        }

        if (first_code_line) {
            print_fence($0);
            first_code_line = 0;
        }

        print;
        if (blank_after_use && $0 == "use proptest::prelude::*;") {
            print "";
            blank_after_use = 0;
        }
        if (snip && $0 == "proptest! {") {
            print "    // snip...";
            print "";
        }
        next;
    }

    !in_code && /^#+ / {
        print "#" $0;
        next;
    }

    {
        print;
    }
' \
    ../book/src/intro.md \
    ../book/src/proptest/getting-started.md \
    ../book/src/proptest/vs-quickcheck.md \
    ../book/src/proptest/limitations.md |
    sed 's#http://hypothesis.works/articles/#https://hypothesis.works/articles/#' >>README.md
cat readme-antelogue.md >>README.md
