# Port forwarder with a UI

# Configuration UI
![image](https://github.com/wushilin/portforwarder_rs/assets/7019828/71317719-e2ef-4d91-984a-e6dab3b18851)

# Online configuration change
![image](https://github.com/wushilin/portforwarder_rs/assets/7019828/ba2efaaa-3bef-4648-95a4-74b61d469473)

# Realtime monitoring and statistics

Supports

- Total connection
- Active connection
- Uploaded bytes
- Downloaded bytes

![image](https://github.com/wushilin/portforwarder_rs/assets/7019828/7ce3a32c-8a6b-42a1-8ff2-ed3b4c2f969a)

# Building

```bash
$ cargo build --release
```

# Running
## Directory structure

You should build your port forwarder using `$ cargo build --release`

And copy `target/release/portforwarder` to a separate folder.

In the same folder, you should also copy the following files:

- static/
- config.yaml
- log4rs.yaml


## Prepare configuration

Example config.yaml
```yaml
# define your listeners
listeners:
  google: # listener name
    bind: 0.0.0.0:1443  # listener bind address and port
    targets:
    - www.google.com:443  # forward to www.google.com:443
options:
  health_check_timeout_ms: 4000 # Targets will be health checked. Not working hosts will be removed from targets temporarily, unless they come online again
  log_config_file: log4rs.yaml # log config file
  max_idle_time_ms: 1000000 # connection can remain open and idle for 1000 seconds (no data transferred means idling)
dns:
  "www.googlex.com:443": "www.google.com:443" # if target is www.googlex.com:443, we will redirect to connect to www.google.com:443 instead
admin_server: # Admin UI
  bind_address: 0.0.0.0 # bind on all interfaces 
  bind_port: 48889 # bind on port 48889
  username: admin # Basic username: admin
  password: pass1234 # Basic password: pass1234
  tls_cert: null # Do not enable TLS. If you enable TLS, you need to put your PEM path here
  tls_key: null # Private key PEM file path
  tls_ca_cert: null # Certificate Authority cert PEM file path
  mutual_tls: null # If set to `true`, mutual TLS will be required
  tls: false # if set to `true` TLS will be used
  rocket_log_level: normal # Rocket log level. Default is normal
```

Sample log4rs.yaml
```yaml
refresh_rate: 60 seconds

appenders:
  stdout:
    kind: console
  default:
    kind: rolling_file
    path: "portforwarder.log"
    append: true
    encoder:
      pattern: "{d(%Y-%m-%d %H:%M:%S%.3f %Z)} {M} {({l}):5.5} {f}:{L} - {m}{n}"
    policy:
      kind: compound
      trigger:
        kind: size
        limit: 10 mb
      roller:
        kind: fixed_window
        pattern: "portforwarder.{}.log.gz"
        count: 20
        base: 1
root:
  level: info
  appenders:
    - default
    - stdout

loggers:
  portforwarder:
    level: info
    appenders:
      - default
      - stdout
    additive: false
```

Sample systemd unit file
```yaml
[Unit]
Description=The port forwarder
After=syslog.target network-online.target remote-fs.target nss-lookup.target
Wants=network-online.target
        
[Service]
Type=simple
WorkingDirectory=/opt/services/portforwarder
PIDFile=/opt/services/portforwarder/portforwarder.pid
ExecStart=/opt/services/portforwarder/portforwarder
# ExecStop=/bin/kill -s QUIT $MAINPID
PrivateTmp=true
        
[Install]
WantedBy=multi-user.target
```


