# tunnel server configuration (large demo); secrets show as fingerprints or ${ENV} names
listen = "0.0.0.0:7835"
public_address = "tunnel.example.com"
insecure_plaintext = false
update {
  remote = true
}
websocket {
  listen = "127.0.0.1:7836"
  path = "/tunnel"
  host = "tunnel.example.com"
  keepalive_secs = 20
  challenge = "http"
}
heartbeat {
  interval_secs = 15
}
limits {
  handshake_timeout_secs = 5
  max_handshakes = 4096
  max_connections_per_client = 8192
  max_pending = 512
}
pool {
  min_idle = 4
  max_idle = 64
}
tcp {
  nodelay = true
  keepalive_secs = 30
}
udp {
  session_timeout_secs = 120
  max_peers = 4096
}
metrics {
  listen = "127.0.0.1:9464"
}
self {
  allow = ["127.0.0.1/32", "10.0.0.0/8"]
}
clients {
  etiolles-pve {
    secret = ["«secret 5818670a»", "«secret 19825b9e»"]
  }
  etiolles-pve-ai {
    secret = "«secret 705fde3f»"
    max_connections = 264
  }
  etiolles-pve-stagging {
    secret = "«secret 48c47811»"
  }
  etiolles-pve-prod {
    secret = ["«secret d6a35514»", "«secret 3785dc8c»"]
  }
  etiolles-ai-server {
    secret = "«secret dec8b341»"
  }
  etiolles-nas {
    secret = "«secret ace3ed09»"
    max_connections = 296
  }
  etiolles-edge {
    secret = ["«secret 543e0f0c»", "«secret 510eaea4»"]
  }
  etiolles-backup {
    secret = "«secret 7119212f»"
  }
  hetzner-pve {
    secret = "«secret 6529649b»"
  }
  hetzner-pve-ai {
    secret = ["«secret 61ffe490»", "«secret 97ce3dd0»"]
    max_connections = 328
  }
  hetzner-pve-stagging {
    secret = "«secret 005f0f8e»"
  }
  hetzner-pve-prod {
    secret = "«secret 5e6af347»"
  }
  hetzner-ai-server {
    secret = ["«secret 91177d88»", "«secret 36740a08»"]
  }
  hetzner-nas {
    secret = "«secret 5eff2d18»"
    max_connections = 360
  }
  hetzner-edge {
    secret = "«secret 42fc647f»"
  }
  hetzner-backup {
    secret = ["«secret 99143070»", "«secret c57cd2b0»"]
  }
  ikoula-pve {
    secret = "«secret 8d766a9c»"
  }
  ikoula-pve-ai {
    secret = "«secret 3a9d4e9d»"
    max_connections = 392
  }
  ikoula-pve-stagging {
    secret = ["«secret 49cfc4ab»", "«secret 18e0a505»"]
  }
  ikoula-pve-prod {
    secret = "«secret bb6c8d0a»"
  }
  ikoula-ai-server {
    secret = "«secret 06a3622f»"
  }
  ikoula-nas {
    secret = ["«secret 5c306bcf»", "«secret 066204b1»"]
    max_connections = 424
  }
  ikoula-edge {
    secret = "«secret db4a597a»"
  }
  ikoula-backup {
    secret = "«secret d07e3c65»"
  }
  scaleway-pve {
    secret = ["«secret fd157892»", "«secret 7915d206»"]
  }
  scaleway-pve-ai {
    secret = "«secret a99f0037»"
    max_connections = 456
  }
  scaleway-pve-stagging {
    secret = "«secret 26a22099»"
  }
  scaleway-pve-prod {
    secret = ["«secret c7a04b8c»", "«secret eb2a4424»"]
  }
  scaleway-ai-server {
    secret = "«secret 3e829d89»"
  }
  scaleway-nas {
    secret = "«secret 078c19a1»"
    max_connections = 488
  }
  scaleway-edge {
    secret = ["«secret b1c7f3a4»", "«secret 24344afc»"]
  }
  scaleway-backup {
    secret = "«secret 2e6fc0d7»"
  }
  ovh-pve {
    secret = "«secret bb5aa80a»"
  }
  ovh-pve-ai {
    secret = ["«secret 3e3be93f»", "«secret 966b9281»"]
    max_connections = 520
  }
  ovh-pve-stagging {
    secret = "«secret 79682b11»"
  }
  ovh-pve-prod {
    secret = "«secret 3921d814»"
  }
  ovh-ai-server {
    secret = ["«secret ebf0ec41»", "«secret 3bea9707»"]
  }
  ovh-nas {
    secret = "«secret 10262e09»"
    max_connections = 552
  }
  ovh-edge {
    secret = "«secret 958a620c»"
  }
  ovh-backup {
    secret = ["«secret 3ef52c2f»", "«secret edc4d311»"]
  }
  gravelines-pve {
    secret = "«secret ccfdc3ff»"
  }
  gravelines-pve-ai {
    secret = "«secret 96e06ef4»"
    max_connections = 584
  }
  gravelines-pve-stagging {
    secret = ["«secret d4208e9a»", "«secret ce9430ce»"]
  }
  gravelines-pve-prod {
    secret = "«secret bf2bfdbb»"
  }
  gravelines-ai-server {
    secret = "«secret e2b436bc»"
  }
  gravelines-nas {
    secret = ["«secret 67a90a64»", "«secret 973bc03c»"]
    max_connections = 616
  }
  gravelines-edge {
    secret = "«secret 63dfc03b»"
  }
  gravelines-backup {
    secret = "«secret 9b8788a4»"
  }
  roubaix-pve {
    secret = ["«secret 71c21f1b»", "«secret 6db4fe55»"]
  }
  roubaix-pve-ai {
    secret = "«secret 6c260810»"
    max_connections = 648
  }
  roubaix-pve-stagging {
    secret = "«secret 7538170e»"
  }
  roubaix-pve-prod {
    secret = ["«secret fbfa82c7»", "«secret 05cef8c9»"]
  }
  roubaix-ai-server {
    secret = "«secret 1a166408»"
  }
  roubaix-nas {
    secret = "«secret 6b97e798»"
    max_connections = 680
  }
  roubaix-edge {
    secret = ["«secret 9769fbff»", "«secret e56f35c1»"]
  }
  roubaix-backup {
    secret = "«secret a33a53f0»"
  }
  strasbourg-pve {
    secret = "«secret 7544bd7b»"
  }
  strasbourg-pve-ai {
    secret = ["«secret 56df5170»", "«secret 345df5b0»"]
    max_connections = 712
  }
  strasbourg-pve-stagging {
    secret = "«secret aac6076e»"
  }
  strasbourg-pve-prod {
    secret = "«secret 224ec727»"
  }
  strasbourg-ai-server {
    secret = ["«secret 00fc93e8»", "«secret af525a68»"]
  }
  strasbourg-nas {
    secret = "«secret 6ca01cf8»"
    max_connections = 744
  }
  strasbourg-edge {
    secret = "«secret be081edf»"
  }
  strasbourg-backup {
    secret = ["«secret 014d42d0»", "«secret 3c9ce790»"]
  }
  frankfurt-pve {
    secret = "«secret 3b52dc2a»"
  }
  frankfurt-pve-ai {
    secret = "«secret 655425df»"
    max_connections = 776
  }
  frankfurt-pve-stagging {
    secret = ["«secret c12507b1»", "«secret 59b67ed7»"]
  }
  frankfurt-pve-prod {
    secret = "«secret 479989b4»"
  }
  frankfurt-ai-server {
    secret = "«secret 93e475e1»"
  }
  frankfurt-nas {
    secret = ["«secret 92973829»", "«secret 77e3922f»"]
    max_connections = 808
  }
  frankfurt-edge {
    secret = "«secret e6c9a2ac»"
  }
  frankfurt-backup {
    secret = "«secret 2081a2cf»"
  }
  helsinki-pve {
    secret = ["«secret e2f643b6»", "«secret a9a1c632»"]
  }
  helsinki-pve-ai {
    secret = "«secret 3a98dceb»"
    max_connections = 840
  }
  helsinki-pve-stagging {
    secret = "«secret 289d9ee5»"
  }
  helsinki-pve-prod {
    secret = ["«secret 09af6d60»", "«secret 0500fa00»"]
  }
  helsinki-ai-server {
    secret = "«secret f375867d»"
  }
  helsinki-nas {
    secret = "«secret 3e1ad54d»"
    max_connections = 872
  }
  helsinki-edge {
    secret = ["«secret 8d1eadc0»", "«secret 6b6729e0»"]
  }
  helsinki-backup {
    secret = "«secret f653d4bb»"
  }
  amsterdam-pve {
    secret = "«secret ef286f8d»"
  }
  amsterdam-pve-ai {
    secret = ["«secret acbac90e»", "«secret b15bc64a»"]
    max_connections = 904
  }
  amsterdam-pve-stagging {
    secret = "«secret 02fa1310»"
  }
  amsterdam-pve-prod {
    secret = "«secret a7a10d5d»"
  }
  amsterdam-ai-server {
    secret = ["«secret b5f4b4b6»", "«secret a9c8d932»"]
  }
  amsterdam-nas {
    secret = "«secret 3298100e»"
    max_connections = 936
  }
  amsterdam-edge {
    secret = "«secret 06f7fc5d»"
  }
  amsterdam-backup {
    secret = ["«secret d4394126»", "«secret 5106d402»"]
  }
  london-pve {
    secret = "«secret 57909735»"
  }
  london-pve-ai {
    secret = "«secret 0d45c246»"
    max_connections = 968
  }
  london-pve-stagging {
    secret = ["«secret 945a3e18»", "«secret f00d7b38»"]
  }
  london-pve-prod {
    secret = "«secret ea2cbc95»"
  }
  london-ai-server {
    secret = "«secret 6530fc7e»"
  }
  london-nas {
    secret = ["«secret 687db766»", "«secret d7030dc2»"]
    max_connections = 1000
  }
  london-edge {
    secret = "«secret 68636d75»"
  }
  london-backup {
    secret = "«secret 1d73e68e»"
  }
}
visitors {
  victor-pc {
    secret = "«secret 321efaf1»"
    admin = "write"
    reach = {}
  }
  victor-laptop {
    secret = "«secret 05b5759c»"
    admin = "read"
    reach {
      etiolles-pve-ai = ["hetzner-pve-ssh", "hetzner-pve-web"]
      etiolles-backup = "self"
    }
  }
  wendy-pc {
    secret = "«secret ad31e83b»"
    max_connections = 34
  }
  wendy-phone {
    secret = "«secret 7236477e»"
    reach = {}
    admin_from = ["10.3.0.0/16", "192.168.3.0/24"]
  }
  ops-oncall {
    secret = "«secret e1e1bb70»"
    reach {
      etiolles-ai-server = ["ovh-pve-ssh", "ovh-pve-web"]
      scaleway-ai-server = "self"
    }
  }
  ci-runner {
    secret = "«secret dc5c1f47»"
    admin = "write"
  }
  grafana-bot {
    secret = "«secret 20849c82»"
    admin = "read"
    reach = {}
    max_connections = 38
  }
  backup-agent {
    secret = "«secret 5aa94430»"
    reach {
      etiolles-backup = ["strasbourg-pve-ssh", "strasbourg-pve-web"]
      roubaix-pve-ai = "self"
    }
  }
  alice-laptop {
    secret = "«secret 012e84ed»"
  }
  bob-desktop {
    secret = "«secret bb82a320»"
    reach = {}
    admin_from = ["10.9.0.0/16", "192.168.9.0/24"]
  }
  carol-tablet {
    secret = "«secret 6b7d2fd2»"
    admin = "write"
    reach {
      hetzner-pve-stagging = ["amsterdam-pve-ssh", "amsterdam-pve-web"]
      frankfurt-edge = "self"
    }
    max_connections = 42
  }
  dave-workstation {
    secret = "«secret 747ab21e»"
    admin = "read"
  }
  erin-laptop {
    secret = "«secret d195fb41»"
    reach = {}
  }
  frank-pc {
    secret = "«secret b9b4c514»"
    reach {
      hetzner-nas = ["hetzner-pve-ssh", "hetzner-pve-web"]
      london-pve-prod = "self"
    }
  }
  grace-phone {
    secret = "«secret 6fd7bcff»"
    max_connections = 46
  }
  heidi-laptop {
    secret = "«secret 0f17f786»"
    admin = "write"
    reach = {}
    admin_from = ["10.15.0.0/16", "192.168.15.0/24"]
  }
  ivan-desktop {
    secret = "«secret 7cd17faf»"
    admin = "read"
    reach {
      ikoula-pve = "self"
    }
  }
  judy-laptop {
    secret = "«secret e848d043»"
  }
  mallory-audit {
    secret = "«secret 93ec7b1e»"
    reach = {}
    max_connections = 50
  }
  niaj-laptop {
    secret = "«secret 422c427b»"
    reach {
      ikoula-pve-prod = ["strasbourg-pve-ssh", "strasbourg-pve-web"]
      ovh-nas = "self"
    }
  }
  olivia-pc {
    secret = "«secret 4fd749e2»"
    admin = "write"
  }
  peggy-laptop {
    secret = "«secret cbcb5635»"
    admin = "read"
    reach = {}
    admin_from = ["10.21.0.0/16", "192.168.21.0/24"]
  }
  rupert-desktop {
    secret = "«secret bd1f8327»"
    reach {
      ikoula-edge = ["amsterdam-pve-ssh", "amsterdam-pve-web"]
      strasbourg-pve-stagging = "self"
    }
    max_connections = 54
  }
  sybil-laptop {
    secret = "«secret ef2ab5c8»"
  }
  trent-admin {
    secret = "«secret 8c164047»"
    reach = {}
  }
}
services {
  etiolles-pve-ssh {
    client = "etiolles-pve"
    target = 22
    visitors = ["victor-pc", "victor-laptop"]
  }
  etiolles-pve-web {
    target = 8006
    clients = ["etiolles-pve", "etiolles-pve-ai"]
    mode = "failover"
    visitors = ["victor-pc", "wendy-pc"]
    health_check {
      interval_secs = 10
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  etiolles-vm-ssh {
    bind = 65001
    target = "192.168.50.142:22"
    clients = ["etiolles-pve", "etiolles-pve-ai"]
    allow_from = ["10.0.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  etiolles-wireguard {
    target = "192.168.50.164:51820"
    bind = 65002
    protocol = "udp"
    clients = ["etiolles-pve", "etiolles-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "10mbit"
  }
  etiolles-wgdashboard {
    target = "192.168.50.164:10086"
    clients = [
      {
        name = "etiolles-pve"
        target = 10086
        weight = 10
      },
      {
        name = "etiolles-pve-ai"
        target = "192.168.50.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  etiolles-backup-sync {
    client = "etiolles-backup"
    target = "192.168.50.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  hetzner-pve-ssh {
    client = "hetzner-pve"
    target = 22
    visitors = ["victor-laptop", "wendy-pc"]
  }
  hetzner-pve-web {
    target = 8006
    clients = ["hetzner-pve", "hetzner-pve-ai"]
    mode = "balance"
    visitors = ["victor-pc", "wendy-phone"]
    health_check {
      interval_secs = 11
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  hetzner-vm-ssh {
    bind = 65003
    target = "192.168.51.142:22"
    clients = ["hetzner-pve", "hetzner-pve-ai"]
    allow_from = ["10.1.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  hetzner-wireguard {
    target = "192.168.51.164:51820"
    bind = 65004
    protocol = "udp"
    clients = ["hetzner-pve", "hetzner-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "20mbit"
  }
  hetzner-wgdashboard {
    target = "192.168.51.164:10086"
    clients = [
      {
        name = "hetzner-pve"
        target = 10086
        weight = 11
      },
      {
        name = "hetzner-pve-ai"
        target = "192.168.51.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  hetzner-backup-sync {
    client = "hetzner-backup"
    target = "192.168.51.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  ikoula-pve-ssh {
    client = "ikoula-pve"
    target = 22
    visitors = ["wendy-pc", "wendy-phone"]
  }
  ikoula-pve-web {
    target = 8006
    clients = ["ikoula-pve", "ikoula-pve-ai"]
    mode = "least_connections"
    visitors = ["victor-pc", "ops-oncall"]
    health_check {
      interval_secs = 12
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  ikoula-vm-ssh {
    bind = 65005
    target = "192.168.52.142:22"
    clients = ["ikoula-pve", "ikoula-pve-ai"]
    allow_from = ["10.2.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  ikoula-wireguard {
    target = "192.168.52.164:51820"
    bind = 65006
    protocol = "udp"
    clients = ["ikoula-pve", "ikoula-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "30mbit"
  }
  ikoula-wgdashboard {
    target = "192.168.52.164:10086"
    clients = [
      {
        name = "ikoula-pve"
        target = 10086
        weight = 12
      },
      {
        name = "ikoula-pve-ai"
        target = "192.168.52.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  ikoula-backup-sync {
    client = "ikoula-backup"
    target = "192.168.52.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  scaleway-pve-ssh {
    client = "scaleway-pve"
    target = 22
    visitors = ["wendy-phone", "ops-oncall"]
  }
  scaleway-pve-web {
    target = 8006
    clients = ["scaleway-pve", "scaleway-pve-ai"]
    mode = "ip_hash"
    visitors = ["victor-pc", "ci-runner"]
    health_check {
      interval_secs = 13
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  scaleway-vm-ssh {
    bind = 65007
    target = "192.168.53.142:22"
    clients = ["scaleway-pve", "scaleway-pve-ai"]
    allow_from = ["10.3.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  scaleway-wireguard {
    target = "192.168.53.164:51820"
    bind = 65008
    protocol = "udp"
    clients = ["scaleway-pve", "scaleway-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "40mbit"
  }
  scaleway-wgdashboard {
    target = "192.168.53.164:10086"
    clients = [
      {
        name = "scaleway-pve"
        target = 10086
        weight = 13
      },
      {
        name = "scaleway-pve-ai"
        target = "192.168.53.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  scaleway-backup-sync {
    client = "scaleway-backup"
    target = "192.168.53.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  ovh-pve-ssh {
    client = "ovh-pve"
    target = 22
    visitors = ["ops-oncall", "victor-pc"]
  }
  ovh-pve-web {
    target = 8006
    clients = ["ovh-pve", "ovh-pve-ai"]
    mode = "failover"
    visitors = ["victor-pc", "grafana-bot"]
    health_check {
      interval_secs = 14
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  ovh-vm-ssh {
    bind = 65009
    target = "192.168.54.142:22"
    clients = ["ovh-pve", "ovh-pve-ai"]
    allow_from = ["10.4.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  ovh-wireguard {
    target = "192.168.54.164:51820"
    bind = 65010
    protocol = "udp"
    clients = ["ovh-pve", "ovh-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "50mbit"
  }
  ovh-wgdashboard {
    target = "192.168.54.164:10086"
    clients = [
      {
        name = "ovh-pve"
        target = 10086
        weight = 14
      },
      {
        name = "ovh-pve-ai"
        target = "192.168.54.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  ovh-backup-sync {
    client = "ovh-backup"
    target = "192.168.54.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  gravelines-pve-ssh {
    client = "gravelines-pve"
    target = 22
    visitors = ["victor-pc", "victor-laptop"]
  }
  gravelines-pve-web {
    target = 8006
    clients = ["gravelines-pve", "gravelines-pve-ai"]
    mode = "balance"
    visitors = ["victor-pc", "backup-agent"]
    health_check {
      interval_secs = 15
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  gravelines-vm-ssh {
    bind = 65011
    target = "192.168.55.142:22"
    clients = ["gravelines-pve", "gravelines-pve-ai"]
    allow_from = ["10.5.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  gravelines-wireguard {
    target = "192.168.55.164:51820"
    bind = 65012
    protocol = "udp"
    clients = ["gravelines-pve", "gravelines-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "60mbit"
  }
  gravelines-wgdashboard {
    target = "192.168.55.164:10086"
    clients = [
      {
        name = "gravelines-pve"
        target = 10086
        weight = 15
      },
      {
        name = "gravelines-pve-ai"
        target = "192.168.55.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  gravelines-backup-sync {
    client = "gravelines-backup"
    target = "192.168.55.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  roubaix-pve-ssh {
    client = "roubaix-pve"
    target = 22
    visitors = ["victor-laptop", "wendy-pc"]
  }
  roubaix-pve-web {
    target = 8006
    clients = ["roubaix-pve", "roubaix-pve-ai"]
    mode = "least_connections"
    visitors = ["victor-pc", "alice-laptop"]
    health_check {
      interval_secs = 16
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  roubaix-vm-ssh {
    bind = 65013
    target = "192.168.56.142:22"
    clients = ["roubaix-pve", "roubaix-pve-ai"]
    allow_from = ["10.6.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  roubaix-wireguard {
    target = "192.168.56.164:51820"
    bind = 65014
    protocol = "udp"
    clients = ["roubaix-pve", "roubaix-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "70mbit"
  }
  roubaix-wgdashboard {
    target = "192.168.56.164:10086"
    clients = [
      {
        name = "roubaix-pve"
        target = 10086
        weight = 16
      },
      {
        name = "roubaix-pve-ai"
        target = "192.168.56.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  roubaix-backup-sync {
    client = "roubaix-backup"
    target = "192.168.56.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  strasbourg-pve-ssh {
    client = "strasbourg-pve"
    target = 22
    visitors = ["wendy-pc", "wendy-phone"]
  }
  strasbourg-pve-web {
    target = 8006
    clients = ["strasbourg-pve", "strasbourg-pve-ai"]
    mode = "ip_hash"
    visitors = ["victor-pc", "bob-desktop"]
    health_check {
      interval_secs = 17
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  strasbourg-vm-ssh {
    bind = 65015
    target = "192.168.57.142:22"
    clients = ["strasbourg-pve", "strasbourg-pve-ai"]
    allow_from = ["10.7.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  strasbourg-wireguard {
    target = "192.168.57.164:51820"
    bind = 65016
    protocol = "udp"
    clients = ["strasbourg-pve", "strasbourg-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "80mbit"
  }
  strasbourg-wgdashboard {
    target = "192.168.57.164:10086"
    clients = [
      {
        name = "strasbourg-pve"
        target = 10086
        weight = 17
      },
      {
        name = "strasbourg-pve-ai"
        target = "192.168.57.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  strasbourg-backup-sync {
    client = "strasbourg-backup"
    target = "192.168.57.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  frankfurt-pve-ssh {
    client = "frankfurt-pve"
    target = 22
    visitors = ["wendy-phone", "ops-oncall"]
  }
  frankfurt-pve-web {
    target = 8006
    clients = ["frankfurt-pve", "frankfurt-pve-ai"]
    mode = "failover"
    visitors = ["victor-pc", "carol-tablet"]
    health_check {
      interval_secs = 18
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  frankfurt-vm-ssh {
    bind = 65017
    target = "192.168.58.142:22"
    clients = ["frankfurt-pve", "frankfurt-pve-ai"]
    allow_from = ["10.8.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  frankfurt-wireguard {
    target = "192.168.58.164:51820"
    bind = 65018
    protocol = "udp"
    clients = ["frankfurt-pve", "frankfurt-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "90mbit"
  }
  frankfurt-wgdashboard {
    target = "192.168.58.164:10086"
    clients = [
      {
        name = "frankfurt-pve"
        target = 10086
        weight = 18
      },
      {
        name = "frankfurt-pve-ai"
        target = "192.168.58.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  frankfurt-backup-sync {
    client = "frankfurt-backup"
    target = "192.168.58.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  helsinki-pve-ssh {
    client = "helsinki-pve"
    target = 22
    visitors = ["ops-oncall", "victor-pc"]
  }
  helsinki-pve-web {
    target = 8006
    clients = ["helsinki-pve", "helsinki-pve-ai"]
    mode = "balance"
    visitors = ["victor-pc", "dave-workstation"]
    health_check {
      interval_secs = 19
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  helsinki-vm-ssh {
    bind = 65019
    target = "192.168.59.142:22"
    clients = ["helsinki-pve", "helsinki-pve-ai"]
    allow_from = ["10.9.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  helsinki-wireguard {
    target = "192.168.59.164:51820"
    bind = 65020
    protocol = "udp"
    clients = ["helsinki-pve", "helsinki-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "100mbit"
  }
  helsinki-wgdashboard {
    target = "192.168.59.164:10086"
    clients = [
      {
        name = "helsinki-pve"
        target = 10086
        weight = 19
      },
      {
        name = "helsinki-pve-ai"
        target = "192.168.59.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  helsinki-backup-sync {
    client = "helsinki-backup"
    target = "192.168.59.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  amsterdam-pve-ssh {
    client = "amsterdam-pve"
    target = 22
    visitors = ["victor-pc", "victor-laptop"]
  }
  amsterdam-pve-web {
    target = 8006
    clients = ["amsterdam-pve", "amsterdam-pve-ai"]
    mode = "least_connections"
    visitors = ["victor-pc", "erin-laptop"]
    health_check {
      interval_secs = 20
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  amsterdam-vm-ssh {
    bind = 65021
    target = "192.168.60.142:22"
    clients = ["amsterdam-pve", "amsterdam-pve-ai"]
    allow_from = ["10.10.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  amsterdam-wireguard {
    target = "192.168.60.164:51820"
    bind = 65022
    protocol = "udp"
    clients = ["amsterdam-pve", "amsterdam-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "110mbit"
  }
  amsterdam-wgdashboard {
    target = "192.168.60.164:10086"
    clients = [
      {
        name = "amsterdam-pve"
        target = 10086
        weight = 20
      },
      {
        name = "amsterdam-pve-ai"
        target = "192.168.60.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v1"
  }
  amsterdam-backup-sync {
    client = "amsterdam-backup"
    target = "192.168.60.20:873"
    health_check = true
    visitors = ["backup-agent", "alice-laptop"]
  }
  london-pve-ssh {
    client = "london-pve"
    target = 22
    visitors = ["victor-laptop", "wendy-pc"]
  }
  london-pve-web {
    target = 8006
    clients = ["london-pve", "london-pve-ai"]
    mode = "ip_hash"
    visitors = ["victor-pc", "frank-pc"]
    health_check {
      interval_secs = 21
      timeout_secs = 3
      failures = 2
      path = "/api2/json/version"
    }
  }
  london-vm-ssh {
    bind = 65023
    target = "192.168.61.142:22"
    clients = ["london-pve", "london-pve-ai"]
    allow_from = ["10.11.0.0/16", "192.168.0.0/16"]
    deny_from = ["10.66.0.0/16", "172.16.0.0/12"]
    max_connections = 64
  }
  london-wireguard {
    target = "192.168.61.164:51820"
    bind = 65024
    protocol = "udp"
    clients = ["london-pve", "london-pve-ai"]
    visitors = ["victor-pc", "victor-laptop"]
    bandwidth = "120mbit"
  }
  london-wgdashboard {
    target = "192.168.61.164:10086"
    clients = [
      {
        name = "london-pve"
        target = 10086
        weight = 21
      },
      {
        name = "london-pve-ai"
        target = "192.168.61.165:10086"
        weight = 1
      },
    ]
    mode = "balance"
    visitors = ["victor-pc", "wendy-pc"]
    proxy_protocol = "v2"
  }
  london-backup-sync {
    client = "london-backup"
    target = "192.168.61.20:873"
    health_check = false
    visitors = ["backup-agent", "alice-laptop"]
  }
  whoami {
    client = "etiolles-pve-prod"
    target = "192.168.0.11:8000"
    web {
      hosts = ["whoami.example.org", "www.whoami.example.org"]
      tls = "auto"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "whoami"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/whoami/api"]
      redirect_from = ["old-whoami.example.org", "whoami.legacy.example.org"]
    }
  }
  speedtest {
    client = "hetzner-pve-prod"
    target = "192.168.0.12:8080"
    web {
      hosts = ["speedtest.example.com", "www.speedtest.example.com"]
      tls = "off"
      compress = false
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "speedtest"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$e05d27e1admin"
        viewer = "$2y$05$c7460e60viewer"
      }
    }
  }
  grafana {
    client = "ikoula-pve-prod"
    target = "192.168.0.13:3000"
    web {
      hosts = ["grafana.example.net", "www.grafana.example.net"]
      tls = "internal"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "grafana"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      forward_auth = "sso"
      rate_limit = 52
    }
  }
  prometheus {
    target = "192.168.0.14:9090"
    web {
      hosts = ["prometheus.example.org", "www.prometheus.example.org"]
      tls = "auto"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "prometheus"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/prometheus/api"]
      redirect_from = ["old-prometheus.example.org", "prometheus.legacy.example.org"]
      target_tls {
        insecure_skip_verify = true
      }
    }
    clients = ["scaleway-pve-prod", "scaleway-edge"]
    mode = "failover"
  }
  loki {
    client = "ovh-pve-prod"
    target = "192.168.0.15:3100"
    web {
      hosts = ["loki.example.com", "www.loki.example.com"]
      tls = "auto"
      compress = true
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "loki"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      target_tls = true
      target_http2 = true
    }
  }
  gitea {
    client = "gravelines-pve-prod"
    target = "192.168.0.16:3000"
    web {
      hosts = ["gitea.example.net", "www.gitea.example.net"]
      tls = "off"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "gitea"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$4ad709e0admin"
        viewer = "$2y$05$ce63b017viewer"
      }
      maintenance = false
      max_body_size = "64MB"
      response_header_timeout_secs = 120
    }
  }
  drone {
    client = "roubaix-pve-prod"
    target = "192.168.0.17:8000"
    web {
      hosts = ["drone.example.org", "www.drone.example.org"]
      tls = "internal"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "drone"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/drone/api"]
      redirect_from = ["old-drone.example.org", "drone.legacy.example.org"]
    }
  }
  nextcloud {
    target = "192.168.0.18:80"
    web {
      hosts = ["nextcloud.example.com", "www.nextcloud.example.com"]
      tls = "auto"
      compress = false
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "nextcloud"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      forward_auth = "sso"
      rate_limit = 57
      client_auth {
        ca = "certs/nextcloud-clients-ca.pem"
        crl = "certs/nextcloud.crl"
      }
    }
    clients = ["strasbourg-pve-prod", "strasbourg-edge"]
    mode = "failover"
  }
  vaultwarden {
    client = "frankfurt-pve-prod"
    target = "192.168.0.19:8081"
    web {
      hosts = ["vaultwarden.example.net", "www.vaultwarden.example.net"]
      tls = "auto"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "vaultwarden"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
    }
  }
  jellyfin {
    client = "helsinki-pve-prod"
    target = "192.168.0.20:8096"
    web {
      hosts = ["jellyfin.example.org", "www.jellyfin.example.org"]
      tls = "off"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "jellyfin"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/jellyfin/api"]
      redirect_from = ["old-jellyfin.example.org", "jellyfin.legacy.example.org"]
      basic_auth {
        admin = "$2y$05$d2f1fb57admin"
        viewer = "$2y$05$84b397e2viewer"
      }
      target_tls {
        insecure_skip_verify = true
      }
    }
  }
  immich {
    client = "amsterdam-pve-prod"
    target = "192.168.0.21:2283"
    web {
      hosts = ["immich.example.com", "www.immich.example.com"]
      tls = "internal"
      compress = true
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "immich"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      target_tls = true
      target_http2 = true
    }
  }
  paperless {
    target = "192.168.0.22:8000"
    web {
      hosts = ["paperless.example.net", "www.paperless.example.net"]
      tls = "auto"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "paperless"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
    }
    clients = ["london-pve-prod", "london-edge"]
    mode = "failover"
  }
  homeassistant {
    client = "etiolles-pve-prod"
    target = "192.168.0.23:8123"
    web {
      hosts = ["homeassistant.example.org", "www.homeassistant.example.org"]
      tls = "auto"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "homeassistant"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/homeassistant/api"]
      redirect_from = ["old-homeassistant.example.org", "homeassistant.legacy.example.org"]
      forward_auth = "sso"
      rate_limit = 62
      maintenance = false
      max_body_size = "64MB"
      response_header_timeout_secs = 120
    }
  }
  uptime {
    client = "hetzner-pve-prod"
    target = "192.168.0.24:3001"
    web {
      hosts = ["uptime.example.com", "www.uptime.example.com"]
      tls = "off"
      compress = false
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "uptime"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$f55188e2admin"
        viewer = "$2y$05$1c6c9485viewer"
      }
    }
  }
  minio {
    client = "ikoula-pve-prod"
    target = "192.168.0.25:9001"
    web {
      hosts = ["minio.example.net", "www.minio.example.net"]
      tls = "internal"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "minio"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
    }
  }
  keycloak {
    target = "192.168.0.26:8443"
    web {
      hosts = ["keycloak.example.org", "www.keycloak.example.org"]
      tls = "auto"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "keycloak"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/keycloak/api"]
      redirect_from = ["old-keycloak.example.org", "keycloak.legacy.example.org"]
      target_tls {
        insecure_skip_verify = true
      }
    }
    clients = ["scaleway-pve-prod", "scaleway-edge"]
    mode = "failover"
  }
  outline {
    client = "ovh-pve-prod"
    target = "192.168.0.27:3000"
    web {
      hosts = ["outline.example.com", "www.outline.example.com"]
      tls = "auto"
      compress = true
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "outline"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      target_tls = true
      target_http2 = true
      client_auth {
        ca = "certs/outline-clients-ca.pem"
        crl = "certs/outline.crl"
      }
    }
  }
  plausible {
    client = "gravelines-pve-prod"
    target = "192.168.0.28:8000"
    web {
      hosts = ["plausible.example.net", "www.plausible.example.net"]
      tls = "off"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "plausible"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$8ecd8765admin"
        viewer = "$2y$05$9b92d5f4viewer"
      }
      forward_auth = "sso"
      rate_limit = 67
    }
  }
  umami {
    client = "roubaix-pve-prod"
    target = "192.168.0.29:3000"
    web {
      hosts = ["umami.example.org", "www.umami.example.org"]
      tls = "internal"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "umami"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/umami/api"]
      redirect_from = ["old-umami.example.org", "umami.legacy.example.org"]
    }
  }
  n8n {
    target = "192.168.0.30:5678"
    web {
      hosts = ["n8n.example.com", "www.n8n.example.com"]
      tls = "auto"
      compress = false
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "n8n"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      maintenance = false
      max_body_size = "64MB"
      response_header_timeout_secs = 120
    }
    clients = ["strasbourg-pve-prod", "strasbourg-edge"]
    mode = "failover"
  }
  portainer {
    client = "frankfurt-pve-prod"
    target = "192.168.0.31:9443"
    web {
      hosts = ["portainer.example.net", "www.portainer.example.net"]
      tls = "auto"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "portainer"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
    }
  }
  traefik {
    client = "helsinki-pve-prod"
    target = "192.168.0.32:8082"
    web {
      hosts = ["traefik.example.org", "www.traefik.example.org"]
      tls = "off"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "traefik"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/traefik/api"]
      redirect_from = ["old-traefik.example.org", "traefik.legacy.example.org"]
      basic_auth {
        admin = "$2y$05$6d82aa80admin"
        viewer = "$2y$05$202f66b7viewer"
      }
      target_tls {
        insecure_skip_verify = true
      }
    }
  }
  pihole {
    client = "amsterdam-pve-prod"
    target = "192.168.0.33:80"
    web {
      hosts = ["pihole.example.com", "www.pihole.example.com"]
      tls = "internal"
      compress = true
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "pihole"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      forward_auth = "sso"
      rate_limit = 72
      target_tls = true
      target_http2 = true
    }
  }
  adguard {
    target = "192.168.0.34:3000"
    web {
      hosts = ["adguard.example.net", "www.adguard.example.net"]
      tls = "auto"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "adguard"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
    }
    clients = ["london-pve-prod", "london-edge"]
    mode = "failover"
  }
  wikijs {
    client = "etiolles-pve-prod"
    target = "192.168.0.35:3000"
    web {
      hosts = ["wikijs.example.org", "www.wikijs.example.org"]
      tls = "auto"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "wikijs"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/wikijs/api"]
      redirect_from = ["old-wikijs.example.org", "wikijs.legacy.example.org"]
    }
  }
  bookstack {
    client = "hetzner-pve-prod"
    target = "192.168.0.36:80"
    web {
      hosts = ["bookstack.example.com", "www.bookstack.example.com"]
      tls = "off"
      compress = false
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "bookstack"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$a64e822dadmin"
        viewer = "$2y$05$9c4747ccviewer"
      }
      client_auth {
        ca = "certs/bookstack-clients-ca.pem"
        crl = "certs/bookstack.crl"
      }
    }
  }
  mealie {
    client = "ikoula-pve-prod"
    target = "192.168.0.37:9000"
    web {
      hosts = ["mealie.example.net", "www.mealie.example.net"]
      tls = "internal"
      compress = true
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "mealie"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      maintenance = false
      max_body_size = "64MB"
      response_header_timeout_secs = 120
    }
  }
  firefly {
    target = "192.168.0.38:8080"
    web {
      hosts = ["firefly.example.org", "www.firefly.example.org"]
      tls = "auto"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "firefly"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      paths = ["/", "/firefly/api"]
      redirect_from = ["old-firefly.example.org", "firefly.legacy.example.org"]
      forward_auth = "sso"
      rate_limit = 77
      target_tls {
        insecure_skip_verify = true
      }
    }
    clients = ["scaleway-pve-prod", "scaleway-edge"]
    mode = "failover"
  }
  photoprism {
    client = "ovh-pve-prod"
    target = "192.168.0.39:2342"
    web {
      hosts = ["photoprism.example.com", "www.photoprism.example.com"]
      tls = "auto"
      compress = true
      hsts = false
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "photoprism"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      target_tls = true
      target_http2 = true
    }
  }
  syncthing {
    client = "gravelines-pve-prod"
    target = "192.168.0.40:8384"
    web {
      hosts = ["syncthing.example.net", "www.syncthing.example.net"]
      tls = "off"
      compress = false
      hsts = true
      request_headers {
        X-Forwarded-Proto = "https"
        X-Tunnel-Service = "syncthing"
      }
      response_headers {
        X-Frame-Options = "SAMEORIGIN"
        Referrer-Policy = "strict-origin-when-cross-origin"
      }
      basic_auth {
        admin = "$2y$05$897f4e1fadmin"
        viewer = "$2y$05$e5c4725aviewer"
      }
    }
  }
  status-page {
    client = "hetzner-pve-prod"
    target = "192.168.0.90:80"
    web = "status.example.org"
  }
  docs-site {
    client = "hetzner-pve-prod"
    target = "192.168.0.91:80"
    web = ["docs.example.org", "docs.example.com"]
  }
  self-metrics {
    client = "self"
    target = "127.0.0.1:9100"
  }
}
web {
  http = "127.0.0.1:8080"
  https = 443
  https_redirect = true
  http2 = true
  rate_limit = 200
  request_header_timeout_secs = 30
  idle_timeout_secs = 300
  request_id = true
  hsts = true
  compress = true
  max_body_size = "32MB"
  response_header_timeout_secs = 60
  error_pages = "errors/"
  trusted_proxies = ["10.0.0.0/8", "192.168.0.0/16"]
  proxy_protocol_from = ["10.10.0.1", "10.10.0.2"]
  access_log {
    file = "logs/access.log"
    format = "json"
  }
  forward_auth {
    sso {
      service = "keycloak"
      path = "/realms/tunnel/protocol/openid-connect/auth"
      copy_headers = ["X-Auth-User", "X-Auth-Email"]
    }
    basic {
      service = "outline"
      path = "/auth/verify"
      copy_headers = ["X-User", "X-Groups"]
    }
  }
  redirect_names = {
    "old.example.org" = "example.org"
    "legacy.example.com" = {
      to = "example.com"
      permanent = true
    }
    "beta.example.net" = {
      to = "example.net"
      permanent = false
    }
  }
  request_headers {
    X-Served-By = "tunnel"
    X-Region = "eu-west"
  }
  response_headers {
    Server = "tunnel"
    X-Content-Type-Options = "nosniff"
  }
  certificates = [
    {
      hosts = ["internal.example.org", "intranet.example.org"]
      cert = "certs/internal/fullchain.pem"
      key = "certs/internal/privkey.pem"
    },
    {
      hosts = ["vpn.example.com", "wg.example.com"]
      upload = "vpn-cert"
    },
  ]
  letsencrypt {
    email = "paradoxngwasi@outlook.com"
    directory = "production"
    profile = "tlsserver"
    dns = [
      {
        domains = ["example.org", "example.com"]
        wildcards = ["*.example.org", "*.example.com"]
        cloudflare = {
          token = "$${CLOUDFLARE_API_TOKEN}"
        }
        wait_secs = 60
        resolvers = ["1.1.1.1:53", "1.0.0.1:53"]
      },
      {
        domains = ["example.net", "lab.example.net"]
        rfc2136 = {
          server = "ns1.example.net:53"
          key_name = "tunnel-acme"
          secret = "$${RFC2136_SECRET}"
          algorithm = "hmac-sha256"
        }
        wait_secs = 120
      },
      {
        domains = ["acme.example.com", "edge.example.com"]
        acme_dns = {
          api = "https://auth.acme-dns.io"
          username = "tunnel"
          password = "$${ACME_DNS_PASSWORD}"
          subdomain = "d420c923-bbd7-4056-ab64-c3ca54c9b3cf"
        }
      },
    ]
  }
}
