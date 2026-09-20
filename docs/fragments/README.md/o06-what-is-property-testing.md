### What is property testing?

_Property testing_ is a system of testing code by checking that certain
properties of its output or behaviour are fulfilled for all inputs. These
inputs are generated automatically, and, critically, when a failing input
is found, the input is automatically reduced to a _minimal_ test case.

Property testing is best used to complement traditional unit testing (i.e.,
using specific inputs chosen by hand). Traditional tests can test specific
known edge cases, simple inputs, and inputs that were known in the past to
reveal bugs, whereas property tests will search for more complicated inputs
that cause problems.
