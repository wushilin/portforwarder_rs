# portforwarder_rs
Rust version of portforwarder built on top of tokio async io

# Build
You need to have rust component `x86_64-unknown-linux-musl` to build a static linked binary. You may choose GLIBC dynamic linked library too.

You can add the target by running

```bash
$ rustup target add x86_64-unknown-linux-musl
```

To build static binary:
```bash
$ sh ./build.sh
```

To build dynamic binary (your runtime may fail if no compatible glibc found)

```bash
$ cargo build --release
```

# Running
```bash
$ target/x86_64-unknown-linux-musl/release/portforwarder -b 0.0.0.0:443::www.google.com:443 -b 0.0.0.0:22::www.remote.com:22
```

Unlike the go variant, the bind-address must be specified.
# Getting help

```bash
$ target/x86_64-unknown-linux-musl/release/portforwarder --help
Usage: portforwarder [OPTIONS]

Options:
  -b, --bind <BIND>            
  -r, --ri <RI>                [default: 30000]
      --log-level <LOG_LEVEL>  [default: ]
  -h, --help                   Print help
```

You can specify multiple `-b` flags.

*-r | --ri* means statistics reporting interval in milliseconds

*--log-level* Sets log level. The default is INFO. Possible values: WARN, ERROR, DEBUG and INFO

# Log rotation
Please see github.com/wushilin/logdrs

