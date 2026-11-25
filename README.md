# yantra-sql

A SQL database that gives the same answers as SQLite, with Parquet storage.

AI coding agents write this code.
The [yantra](https://github.com/bettersandeep/yantra) factory gives them the work, checks each change, and decides what merges.

## Rule of this repo

The first commit is a seed made by hand: an empty crate, CI and the licence.
Each change after the seed arrives as a pull request from the factory.

## Design decisions

- Language: Rust.
- Answers: the same as SQLite, checked with SQLite's public test suite.
- Storage: Parquet files, with a small row area for new and changed rows.

## Status

Seed commit.
The database answers no query yet.
