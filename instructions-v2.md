#Follower Maze Challenge

##Dependencies
1. JDK 7 VM
2. Golang 1.7 or later
**Note:** This implementaiton uses only Golang standard library

##Setup
1. Clone the repo.
2. Setup go workspace appropriately (if not already setup)
3. `cd $GOPATH/github.com/soundcloud/maze-server`
4. `go build`
5. `./maze-server`
6. In a separate terminal, navigate to the same directory and run `./followermaze.sh`

##Server Configurations
The following parameters are configurable:
   - **logLevel**: Set log level for custom logger.
   - **clientListenerPort**: The Port the server will listen for user clients on.
   - **eventListenerPort**: The Port the server will listen for events on.
   - **sequenceNumber**: The sequence number of the first event the server should expect to receive.

These coniguration parameters can be set in the `conf.json` file, or passed in as commandline arguments to the server.
*Example:* <br />
```./maze-server logLevel=DEBUG clientListenerPort=8080 eventListenerPort=3333``` <br />
Any configurations not set by commandline will default to configurations set in `conf.json`. <br />

Please use environment variables to set configurations for `./followermaz.sh` program.

##Makefile
The Makefile contains 4 directives. <br />
1. `make fmt` -> runs `go fmt` on current and all subdirectories. <br />
2. `make vet` -> runs `go vet` on current and all subdirectories. <br />
3. `make test` -> runs all tests in current direcotry and subdirectories. <br />
4. `make bench` -> executes shell that runs a few benchmark tests for server performance. <br />
This makefile is where I would additional features dependent on third-party libraries (e.g. structcheck, swagger annotations and doc generation, additional testing libraries, and custom error generation if needed)

##Logging
This implementation includes a custom logger. Options for logging level can be set in `conf.json`.<br />
Options include "All", "Debug", "Info", "Warn", and "Error". <br />
The default configuration is set to "INFO" but can be set to "Debug" for more in-depth look at the program.