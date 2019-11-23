# Follower maze challenge using rust
[![Build Status](https://travis-ci.org/jxs/follower-maze-rust.svg?branch=master)](https://travis-ci.org/jxs/follower-maze-rust)

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


### instructions
1. start the solution with ` cargo run --release`
2. run `./followermaze-v2.sh`
