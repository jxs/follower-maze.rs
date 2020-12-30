# Follower maze challenge using rust
![Build Status](https://github.com/jxs/follower-maze.rs/workflows/.github/workflows/main.yml/badge.svg)

The challenge solved here is to build a system which acts as a socket
server, reading events from an *event source* and forwarding them when
appropriate to *user clients*.

Clients connect through TCP and use the simple protocol described in a
section below. There are two types of clients connecting to the server:

- **One** *event source*: This sends a
stream of events which may or may not require clients to be notified
- **Many** *user clients*: Each one representing a specific user,
these wait for notifications for the events that are relevant to the
user they represent

A full specification is given in the `instructions-v1.md` or `instructions-v2.md`.
This implementation is compatible with both v1 and v2.

This project was first implemented with `tokio` 0.1 and `futures` 0.1 evolving with them to `std::futures` and `tokio` 1.0.\
You can navigate through the commit history to see it's evolution

### instructions
1. start the solution with ` cargo run --release`
2. run `./followermaze-v2.sh`
