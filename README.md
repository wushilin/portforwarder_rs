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
$ target/x86_64-unknown-linux-musl/release/portforwarder -c config.yaml
```

# About `config.yaml`
config.yaml contains a few sections.

## Global options
Global options are configured under `options:` section in the config yaml.

The following global options are supported.

```yaml
options:
  # The loadbalancer will periodically check if backend is healthy. 
  # When unhealthy, backend will be temporarily surpressed until they come online again.
  # This defines the timeout in millisecond for a connect timeout by the health checker
  healthcheck_timeout_ms: 5000

  # How often in milliseconds, that the statics is printed
  reporting_interval_ms: 5000

  # Specifies the DNS override json (see below)
  dns_override_file: resolve.json

  # Specify the full path of the `log4rs.yaml` config file
  log_config_file: log4rs.yaml

  # Connection that had not transferred any data will be terminated after this timeout
  # To disable the timeout mechanism, set this to 0 or negative number (that means infinity)
  max_idle_ms: 600000
```

### What is the `resolve.json`?
It is actually a static mapping of `host:port` to `host:port` in json format.

For example, you map `0.0.0.0:443` to `www.google.com:443`, and you want to resolve `www.google.com:443` to `192.168.44.100:443`
then you can use `resolve.json` to redirect it.

Sample resolve.json
```json
{
    "www.google.com:443": "127.0.0.1:22",
    "www.baidu.com:443": "127.0.0.1:22",
    "www.google.com:80": "192.168.44.100:80"
}
```

### What is the `log4rs.yaml`?
It is the log4rs configuration. Generally, the default works well, you can just change the log path.

An example:
```yaml
refresh_rate: 60 seconds

appenders:
  stdout:
    kind: console
  default:
    kind: rolling_file
    path: "/var/log/portforwarder.log"
    append: true
    encoder:
      pattern: "{d(%Y-%m-%d %H:%M:%S%.3f %Z)} {({l}):5.5} {f}:{L} - {m}{n}"
    policy:
      kind: compound
      trigger:
        kind: size
        limit: 10 mb
      roller:
        kind: fixed_window
        pattern: "/var/log/tlsproxy.{}.log.gz"
        count: 20
        base: 1
root:
  level: info
  appenders:
    - stdout

loggers:
  portforwarder:
    level: info
    appenders:
      - default
      - stdout
    additive: false
```
## Listeners

Listeners are defined in the listeners section. Each listener has a name and a binding string. 

For example:
```yaml
listeners:
- name: HTTPS 
  bind: 0.0.0.0:80
- name: SSH
  bind: 0.0.0.0:2022
```

`name` is the listener name (must be unique)
`bind` is the listen address (must not repeat)
## Backends

Backends are like target behind load balancer. They will be health checked, and randomly selected.
You can use `resolve.json` to further redirect them as when needed.
```yaml
backends:
- name: bazinga80
  hosts: 
  - 192.168.44.100:80
  - 192.168.44.6:80
  - www.google.com:80
- name: sshbe
  hosts:
  - www.google.com:22
  - 192.168.44.106:22
  - 192.168.44.105:22
```

Each backend has a `name` and one or more `host` entry. The entries will be health checked. Failed hosts will be disabled.

If a backend has all hosts failed, a random host will still be selected and of course the connection will fail....

## Mapping
Mapping binds the listener to the backend.

```yaml
forwarding:
  - from: HTTPS
    to: bazinga80
  - from: SSH
    to: sshbe
```

The `from` is the listener name, the `to` is the backend name.

# Getting help

```bash
$ target/x86_64-unknown-linux-musl/release/portforwarder --help
Usage: portforwarder [OPTIONS]

Options:
  -c, --config-yaml <CONFIG_YAML>  Location of your `config.yaml` [default: config.yaml]
  -h, --help                       Print help
```

# Log rotation
Check your `log4rs.yaml` config.