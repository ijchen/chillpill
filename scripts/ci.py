#!/usr/bin/env python3

"""
Name: CI
Version: 0.1.0
Author(s): Isaac Chen
Description:
    Runs all continuous integration (CI) checks for the codebase. This includes unit and integration
    tests, static code analysis like clippy and rustfmt, ensuring documentation builds, and more.

    This script expects to be run from the root of the repository.

Usage: py scripts/ci.py [STAGE]

Args:
    [STAGE]
        Which stage of CI to perform. Defaults to "all". For a full list of options, see the
        argument parser configuration.

Exit Code:
    0 - Success
    1 - Something went wrong or failed outside this script's control
    2 - Internal script error
    130 - Keyboard interrupt
"""

import argparse
import os
import subprocess
import sys
from typing import Callable, Dict, List, Optional

# Minimum Supported Rust Version (MSRV)
MSRV = "1.85.0"


class ExternalError(Exception):
    """
    Raised when something outside the script's control fails.

    Examples: external command failure, invalid user input, etc.
    Maps to exit code 1.
    """

    pass


def print_header(text: str) -> None:
    """Prints text formatted as a header (colors and arrows and stuff, very cool)"""
    ANSI_BLUE = "\033[1;34m"
    ANSI_RESET = "\033[0m"
    ANSI_BOLD = "\033[1m"

    print(f"{ANSI_BLUE}==>{ANSI_RESET} {ANSI_BOLD}{text}{ANSI_RESET}")


def run_command(cmd: List[str], env: Optional[Dict[str, str]] = None) -> None:
    """
    Run a command, raising ExternalError if it fails.

    Args:
        cmd: Command and arguments as a list
        env: Optional environment variables to add/override

    Raises:
        ExternalError: If the command fails
    """

    # Merge environment variables
    full_env = os.environ.copy()
    if env:
        full_env.update(env)

    try:
        subprocess.run(cmd, check=True, env=full_env)
    except subprocess.CalledProcessError as e:
        # Command failed (no need to print anything, we assume the command printed its own error)
        raise ExternalError from e
    except FileNotFoundError as e:
        # Command not found (e.g., cargo not installed) - this we do need to print
        print(f"Command not found: {cmd[0]}", file=sys.stderr)
        raise ExternalError from e


def check_fmt() -> None:
    """Check code formatting."""
    print_header("Checking code formatting...")
    run_command(
        ["cargo", "+stable", "fmt", "--check"], env={"RUSTFLAGS": "-D warnings"}
    )


def check_docs() -> None:
    """Build documentation with stable and nightly compilers."""
    print_header("Building documentation (stable)...")
    run_command(
        ["cargo", "+stable", "doc", "--document-private-items", "--no-deps"],
        env={"RUSTDOCFLAGS": "-D warnings"},
    )

    print_header("Building documentation (nightly)...")
    run_command(
        ["cargo", "+nightly", "doc", "--document-private-items", "--no-deps"],
        env={"RUSTDOCFLAGS": "-D warnings"},
    )


def lint() -> None:
    """Lint with cargo clippy."""
    print_header("Linting with cargo clippy...")
    run_command(
        [
            "cargo",
            "+stable",
            "clippy",
            "--no-deps",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]
    )


def build() -> None:
    """Run cargo build."""
    print_header("Running cargo build...")
    run_command(
        ["cargo", "+stable", "build", "--all-targets"], env={"RUSTFLAGS": "-D warnings"}
    )


def run_tests_stable() -> None:
    """Run tests with stable compiler."""
    print_header("Running tests (stable compiler)...")
    run_command(["cargo", "+stable", "test"], env={"RUSTFLAGS": "-D warnings"})


def run_tests_beta() -> None:
    """Run tests with beta compiler."""
    print_header("Running tests (beta compiler)...")
    run_command(["cargo", "+beta", "test"], env={"RUSTFLAGS": "-D warnings"})


def run_tests_msrv() -> None:
    """Run tests with MSRV compiler."""
    print_header(f"Running tests (MSRV compiler ({MSRV}))...")
    run_command(["cargo", f"+{MSRV}", "test"], env={"RUSTFLAGS": "-D warnings"})


def run_tests_leak_sanitizer() -> None:
    """Run tests with leak sanitizer."""
    print_header("Running tests with leak sanitizer...")
    run_command(
        ["cargo", "+nightly", "test"],
        env={"RUSTFLAGS": "-D warnings -Z sanitizer=leak"},
    )


# All CI stages, in execution order
CI_STAGES: List[tuple[str, Callable[[], None]]] = [
    ("check_fmt", check_fmt),
    ("check_docs", check_docs),
    ("build", build),
    ("lint", lint),
    ("run_tests_stable", run_tests_stable),
    ("run_tests_beta", run_tests_beta),
    ("run_tests_msrv", run_tests_msrv),
    ("run_tests_leak_sanitizer", run_tests_leak_sanitizer),
]


def all_checks() -> None:
    """Run all CI checks."""
    for name, func in CI_STAGES:
        func()

    print_header("All checks passed! 🎉")


def parse_arguments() -> argparse.Namespace:
    """
    Parse command-line arguments.

    Returns:
        Parsed arguments

    Raises:
        ExternalError: If arguments are invalid (user error)
    """
    parser = argparse.ArgumentParser(
        description="Runs all continuous integration (CI) checks for the codebase.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "stage",
        nargs="?",
        default="all",
        choices=["all"] + [name for name, _ in CI_STAGES],
        help="Which stage of CI to perform (default: all)",
    )

    try:
        return parser.parse_args()
    except SystemExit as e:
        # argparse calls sys.exit() for both errors and --help, but we want to convert argument
        # parsing errors to ExternalError (exit code 1)
        if e.code == 0:
            raise
        else:
            raise ExternalError from e


def validate_environment() -> None:
    """Ensure we're in a Rust project directory."""
    if not os.path.isfile("Cargo.toml"):
        print(
            "Error: Cargo.toml not found. Run this script from the project root.",
            file=sys.stderr,
        )
        raise ExternalError


def main() -> None:
    """Main entry point."""
    args = parse_arguments()

    validate_environment()

    # Dispatch to the appropriate function
    if args.stage == "all":
        all_checks()
    else:
        dict(CI_STAGES)[args.stage]()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nDetected keyboard interrupt, exiting", file=sys.stderr)
        sys.exit(130)
    except ExternalError:
        # Exit code 1: Something outside the script's control failed
        sys.exit(1)
    except Exception:
        # Exit code 2: Internal script error (bug in this script)
        import traceback

        print("\nInternal script error:", file=sys.stderr)
        traceback.print_exc()
        sys.exit(2)
