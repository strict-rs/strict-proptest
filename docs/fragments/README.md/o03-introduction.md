## Introduction

Proptest is a property testing framework (i.e., the QuickCheck family)
inspired by the [Hypothesis](https://hypothesis.works/) framework for
Python. It allows to test that certain properties of your code hold for
arbitrary inputs, and if a failure is found, automatically finds the
minimal test case to reproduce the problem. Unlike QuickCheck, generation
and shrinking is defined on a per-value basis instead of per-type, which
makes it more flexible and simplifies composition.
