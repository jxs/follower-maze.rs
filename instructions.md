# Back-end Developer Challenge: Follower Maze
Thanks for trying our development challenge!

With this document you should have received two other files:

- `followermaze.sh`, an executable bash script
- `FollowerMaze-assembly-1.0.jar`, a JAR file to be executed on a JDK 7 JVM

If you haven't received any of these, or if you think there are any
problems with the files, please contact us immediately and
we will re-send you the missing pieces.

## The Challenge
The challenge proposed here is to build a system which acts as a socket
server, reading events from an *event source* and forwarding them when
appropriate to *user clients*.

Clients will connect through TCP and use the simple protocol described in a
section below. There will be two types of clients connecting to your server:

- **One** *event source*: It will send you a
stream of events which may or may not require clients to be notified
- **Many** *user clients*: Each one representing a specific user,
these wait for notifications for events which would be relevant to the
user they represent

### The Protocol
The protocol used by the clients is string-based (i.e. a `CRLF` control
character terminates each message). All strings are encoded in `UTF-8`.

The *event souce* **connects on port 9090** and will start sending
events as soon as the connection is accepted.

The many *user clients* will **connect on port 9099**. As soon
as the connection is accepted, they will send to the server the ID of
the represented user, so that the server knows which events to
inform them of. For example, once connected a *user client* may send down:
`2932\r\n`, indicating that they are representing user 2932.

After the identification is sent, the *user client* starts waiting for
events to be sent to them. Events coming from *event source* should be
sent to relevant *user clients* exactly like read, no modification is
required or allowed.

### The Events
There are five possible events. The table below describe payloads
sent by the *event source* and what they represent:

| Payload    | Sequence #| Type         | From User Id | To User Id |
|------------|-----------|--------------|--------------|------------|
|666|F|60|50 | 666       | Follow       | 60           | 50         |
|1|U|12|9    | 1         | Unfollow     | 12           | 9          |
|542532|B    | 542532    | Broadcast    | -            | -          |
|43|P|32|56  | 43        | Private Msg  | 2            | 56         |
|634|S|32    | 634       | Status Update| 32           | -          |

Using the verification program supplied, you will receive exactly 1000 events,
with sequence number from 1 to 1000. **The events will arrive out of order**.

*Note: The verification program sends only 1000 events so that you have fast feedback
on your code. Please do not assume that your code would only handle a finite sequence
of events, **we expect your server to handle an arbitrarily large events stream**
(i.e. you would not be able to keep all events in memory or any other storage)*

Events may generate notifications for *user clients*. **If there is a
*user client* ** connected for them, these are the users to be
informed for different event types:

* **Follow**: Only the `To User Id` should be notified
* **Unfollow**: No clients should be notified
* **Broadcast**: All connected *user clients* should be notified
* **Private Message**: Only the `To User Id` should be notified
* **Status Update**: All current followers of the `From User ID` should be notified

If there are no *user client* connected for a user, any notifications
for them must be silently ignored. *user clients* expect to be notified of
events **in the correct order**, regardless of the order in which the
*event source* sent them.

### Your Solution
We expect you to send us the source code of a fully functional server for the
proposed challenge.

The challenge was designed so that a candidate can implement the
solution using just the standard library of most programming languages
and environments (i.e. no external gems/JARs/libs/modules/etc.). Feel
free to use your preferred build and testing libraries, but the production
code should have very minimal dependencies on third-party code,
**preferably none at all**.

Your code should build and run on a Mac or GNU/Linux machine running a
recent OS release.

*As a **non-exhaustive** example, we have received succesful applications
developed on: Node.js, Ruby, JRuby, Haskell, Clojure, Scala, Go, Python,
Java and C/C++.*

If you absolutely think you need some sort of third-party library,
please write a paragraph to help us better understand your choice.

### Before submitting your code
With this document you  received a jar file and a shell script. These
contain one possible implementation of the *event source* and *user
client* described previously.

**We expect you to make sure that your solution works with the
supplied clients before sending it to us**. The first thing we will do
with your code is to run it agains these clients, so you can have very
early feedback by treating it as a test suite.

To run the clients, first make sure you have the server you wrote
running and listening to ports 9090 and 9099, then run:

```
$ ./followermaze.sh
```

This will start the clients, which will immediately start sending
message to your server. You know it finished without errors when it
outputs:

```
 [INFO] ==================================
 [INFO] \o/ ALL NOTIFICATIONS RECEIVED \o/
 [INFO] ==================================
```

### Assesment Criteria
We expect you to write **code you would consider production-ready**.
This means we want your code to be well-factored, without needless
duplication, follow good practices and be automatically verified.

What we will look at:
- If your code fulfils the requirement, and runs against the
supplied example server
- How clean is your design and implementation, how easy it is to
understand and maintain your code
- How you verified your software, if by automated tests or some
other way
