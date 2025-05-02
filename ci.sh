#!/usr/bin/env bash
#
# Name: CI
# Version: 0.1.0
# Author(s): Isaac Chen
# Description:
# > Runs all continuous integration (CI) checks for the codebase. This includes
# > unit and integration tests, static code analysis like clippy and rustfmt,
# > ensuring documentation builds, and more.
# >
# > This script expects to be run directly from the directory it lives in.
# Usage: ci.sh [STAGE]
# Args:
#    [STAGE]
#        Which stage of CI to perform. Defaults to "all". For a full list of
#        options, see the usage of `arg_stage` towards the end of this file.
#
# Exit Code:
#     0 - Success
#     1 - Something went wrong or failed outside this script's control
#     2 - Internal script error

# Fail immediately when something goes wrong
# See: https://gist.github.com/mohanpedala/1e2ff5661761d3abd0385e8223e16425
set -euo pipefail

# Command-line argument parsing
unset arg_stage
while [[ "$#" -gt "0" ]]; do
    case "$1" in
        *)
            # Check for the [STAGE] positional argument
            if ! [[ -v arg_stage ]]; then
                arg_stage="$1"
            else
                echo "Unexpected argument: $1" >&2
                exit 1
            fi
            ;;
    esac

    shift
done

# Script logic

# If the stage argument isn't set, set it to its default value
# See: https://stackoverflow.com/a/28085062
: "${arg_stage:="all"}"

# Prints text formatted as a header (colors and arrows and stuff, very cool)
print_header() {
    echo -e "\e[1;34m==>\e[0m \e[1m$1\e[0m"
}

check_fmt() {
    print_header 'Checking code formatting...'
    RUSTFLAGS='-D warnings' cargo +stable fmt --check
}

check_docs() {
    print_header 'Building documentation (stable)...'
    RUSTDOCFLAGS='-D warnings' cargo +stable doc --document-private-items --no-deps

    print_header 'Building documentation (nightly)...'
    RUSTDOCFLAGS='-D warnings' cargo +nightly doc --document-private-items --no-deps
}

lint() {
    print_header 'Linting with cargo clippy...'
    cargo +stable clippy --no-deps --all-targets -- -D warnings
}

build() {
    print_header 'Running cargo build...'
    RUSTFLAGS='-D warnings' cargo +stable build --all-targets
}

run_tests_stable() {
    print_header 'Running tests (stable compiler)...'
    RUSTFLAGS='-D warnings' cargo +stable test
}

run_tests_beta() {
    print_header 'Running tests (beta compiler)...'
    RUSTFLAGS='-D warnings' cargo +beta test
}

run_tests_msrv() {
    local msrv='1.85.0'

    print_header "Running tests (MSRV compiler ($msrv))..."
    RUSTFLAGS='-D warnings' cargo "+$msrv" test
}

run_tests_leak_sanitizer() {
    print_header 'Running tests with leak sanitizer...'
    RUSTFLAGS='-D warnings -Z sanitizer=leak' cargo +nightly test
}

all_checks() {
    check_fmt
    check_docs
    build
    lint
    run_tests_stable
    run_tests_beta
    run_tests_msrv
    run_tests_leak_sanitizer

    print_header "All checks passed! 🎉"
}

case "$arg_stage" in
    "all")                      all_checks               ;;
    "check_fmt")                check_fmt                ;;
    "check_docs")               check_docs               ;;
    "lint")                     lint                     ;;
    "build")                    build                    ;;
    "run_tests_stable")         run_tests_stable         ;;
    "run_tests_beta")           run_tests_beta           ;;
    "run_tests_msrv")           run_tests_msrv           ;;
    "run_tests_leak_sanitizer") run_tests_leak_sanitizer ;;
    *)
        echo "Unknown stage: $arg_stage"
        echo 'Available stages:'
        echo '    all (default)'
        echo '    check_fmt'
        echo '    check_docs'
        echo '    lint'
        echo '    build'
        echo '    run_tests_stable'
        echo '    run_tests_beta'
        echo '    run_tests_msrv'
        echo '    run_tests_leak_sanitizer'
        exit 1
        ;;
esac
