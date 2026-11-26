// Written by docs/shots/shoot.py --demo, from what the app's own code says
// of hosts that do not exist (src-tauri/tests/doc_data.rs): not by hand.
window.__SHELLDER_DEMO = {
 "state": {
  "version": "2.5.15",
  "hosts": [
   {
    "alias": "bastion",
    "aliases": [],
    "source": "/Users/alice/.ssh/config"
   },
   {
    "alias": "web-1",
    "aliases": [],
    "source": "/Users/alice/.ssh/config"
   },
   {
    "alias": "gpu-box",
    "aliases": [],
    "source": "/Users/alice/.ssh/config"
   },
   {
    "alias": "db-staging",
    "aliases": [],
    "source": "/Users/alice/.ssh/config"
   },
   {
    "alias": "backup",
    "aliases": [],
    "source": "/Users/alice/.ssh/config"
   }
  ],
  "resolved": {
   "backup": {
    "hostname": "backup.example.org",
    "user": "alice",
    "port": "22",
    "proxyJump": null,
    "proxyCommand": null,
    "controlPath": "/Users/alice/.ssh/cm-eeece04e9378dfc193fcf1a249e2d1c14bfeab3f",
    "controlMaster": "auto",
    "controlPersist": "no",
    "identityFiles": [
     "~/.ssh/id_rsa",
     "~/.ssh/id_ecdsa",
     "~/.ssh/id_ecdsa_sk",
     "~/.ssh/id_ed25519",
     "~/.ssh/id_ed25519_sk",
     "~/.ssh/id_mldsa44_ed25519"
    ],
    "serverAliveInterval": 30,
    "strictHostKeyChecking": "ask",
    "jumpAlias": null
   },
   "bastion": {
    "hostname": "bastion.example.org",
    "user": "alice",
    "port": "22",
    "proxyJump": null,
    "proxyCommand": null,
    "controlPath": "/Users/alice/.ssh/cm-fe62ebd19f912b4fe756398e6778d04f83fd62bb",
    "controlMaster": "auto",
    "controlPersist": "no",
    "identityFiles": [
     "~/.ssh/id_rsa",
     "~/.ssh/id_ecdsa",
     "~/.ssh/id_ecdsa_sk",
     "~/.ssh/id_ed25519",
     "~/.ssh/id_ed25519_sk",
     "~/.ssh/id_mldsa44_ed25519"
    ],
    "serverAliveInterval": 30,
    "strictHostKeyChecking": "ask",
    "jumpAlias": null
   },
   "db-staging": {
    "hostname": "10.0.3.40",
    "user": "postgres",
    "port": "22",
    "proxyJump": "bastion",
    "proxyCommand": null,
    "controlPath": "/Users/alice/.ssh/cm-c41148afe0743e780666a6781c721014fe3c39f4",
    "controlMaster": "auto",
    "controlPersist": "no",
    "identityFiles": [
     "~/.ssh/id_rsa",
     "~/.ssh/id_ecdsa",
     "~/.ssh/id_ecdsa_sk",
     "~/.ssh/id_ed25519",
     "~/.ssh/id_ed25519_sk",
     "~/.ssh/id_mldsa44_ed25519"
    ],
    "serverAliveInterval": 30,
    "strictHostKeyChecking": "ask",
    "jumpAlias": "bastion"
   },
   "gpu-box": {
    "hostname": "gpu-box.example.org",
    "user": "alice",
    "port": "2222",
    "proxyJump": null,
    "proxyCommand": null,
    "controlPath": "/Users/alice/.ssh/cm-cd2beb55667bab0cbdc260c76626bf9507747b3b",
    "controlMaster": "auto",
    "controlPersist": "no",
    "identityFiles": [
     "~/.ssh/id_rsa",
     "~/.ssh/id_ecdsa",
     "~/.ssh/id_ecdsa_sk",
     "~/.ssh/id_ed25519",
     "~/.ssh/id_ed25519_sk",
     "~/.ssh/id_mldsa44_ed25519"
    ],
    "serverAliveInterval": 30,
    "strictHostKeyChecking": "ask",
    "jumpAlias": null
   },
   "web-1": {
    "hostname": "10.0.3.11",
    "user": "deploy",
    "port": "22",
    "proxyJump": "bastion",
    "proxyCommand": null,
    "controlPath": "/Users/alice/.ssh/cm-8262e40c5e1ebc3f4280dfd2421a2452aabc3e4f",
    "controlMaster": "auto",
    "controlPersist": "no",
    "identityFiles": [
     "~/.ssh/id_rsa",
     "~/.ssh/id_ecdsa",
     "~/.ssh/id_ecdsa_sk",
     "~/.ssh/id_ed25519",
     "~/.ssh/id_ed25519_sk",
     "~/.ssh/id_mldsa44_ed25519"
    ],
    "serverAliveInterval": 30,
    "strictHostKeyChecking": "ask",
    "jumpAlias": "bastion"
   }
  },
  "resolveErrors": {},
  "statuses": {
   "bastion": {
    "host": "bastion",
    "enabled": true,
    "locked": false,
    "state": {
     "kind": "up",
     "pid": 4242
    },
    "since": 1795589444,
    "lastError": null,
    "quickFailures": 0,
    "idleMode": "none",
    "neededBy": [
     "web-1"
    ],
    "socket": true
   },
   "web-1": {
    "host": "web-1",
    "enabled": true,
    "locked": true,
    "state": {
     "kind": "up",
     "pid": 4242
    },
    "since": 1795589544,
    "lastError": null,
    "quickFailures": 0,
    "idleMode": "none",
    "neededBy": [],
    "socket": true
   },
   "gpu-box": {
    "host": "gpu-box",
    "enabled": true,
    "locked": true,
    "state": {
     "kind": "up",
     "pid": 4242
    },
    "since": 1795664844,
    "lastError": null,
    "quickFailures": 0,
    "idleMode": "none",
    "neededBy": [],
    "socket": true
   },
   "db-staging": {
    "host": "db-staging",
    "enabled": true,
    "locked": true,
    "state": {
     "kind": "waiting",
     "retryIn": 20
    },
    "since": null,
    "lastError": null,
    "quickFailures": 0,
    "idleMode": "none",
    "neededBy": [],
    "socket": true
   },
   "backup": {
    "host": "backup",
    "enabled": false,
    "locked": false,
    "state": {
     "kind": "off"
    },
    "since": null,
    "lastError": null,
    "quickFailures": 0,
    "idleMode": "none",
    "neededBy": [],
    "socket": true
   }
  },
  "enabled": [
   "bastion",
   "web-1",
   "gpu-box",
   "db-staging"
  ],
  "locked": [
   "web-1",
   "gpu-box",
   "db-staging"
  ],
  "secrets": {
   "backup": [],
   "bastion": [],
   "db-staging": [],
   "gpu-box": [
    "password",
    "totp"
   ],
   "web-1": [
    "passphrase"
   ]
  },
  "keychainRefusal": null,
  "configFiles": [
   "/Users/alice/.ssh/config"
  ],
  "reloading": false,
  "prompt": null,
  "pendingPrompts": 0,
  "settings": {
   "keepInBackground": false,
   "showMenuBarIcon": true,
   "invertMenuBarIcon": false,
   "silentLaunch": false,
   "language": "en",
   "launchedLanguage": "en",
   "startAtLogin": true,
   "autoUpdate": true,
   "autostartOtherCopy": null,
   "error": null,
   "platform": "macos",
   "terminal": {
    "font": "",
    "lineHeight": 1.15,
    "cursor": "block",
    "cursorBlink": true,
    "scrollback": 10000,
    "themeLight": "default",
    "themeDark": "default",
    "opacity": 1.0,
    "blurStrength": 0,
    "padding": 2.0,
    "copyOnSelect": false,
    "pasteProtection": true,
    "confirmClose": true,
    "bell": "flash",
    "bellSound": "",
    "notifications": true,
    "noticeSound": "",
    "shellIntegration": true,
    "webgl": false
   },
   "quickConsoleKey": null,
   "quickConsoleHost": null,
   "reconnect": {
    "growth": "doubling",
    "first": 5,
    "max": 600,
    "giveUp": 0
   },
   "hostReconnect": {},
   "logPanelHeight": 170,
   "quickKeyError": null
  },
  "update": {
   "state": "upToDate",
   "version": null,
   "page": null,
   "notes": null,
   "progress": null,
   "message": null
  },
  "platform": {
   "controlMaster": true
  },
  "paths": {
   "sshConfig": "/Users/alice/.ssh/config",
   "log": "/Users/alice/Library/Application Support/shellder/shellder.log",
   "stateDir": "/Users/alice/Library/Application Support/shellder",
   "home": "/Users/alice"
  },
  "constants": {
   "healthInterval": 60,
   "connectTimeout": 300
  },
  "connected": [
   "bastion",
   "web-1",
   "gpu-box"
  ],
  "idleModes": [
   "none",
   "shell",
   "cat"
  ]
 },
 "reading": {
  "hostname": "gpu-box",
  "os": "Ubuntu 24.04.1 LTS",
  "kernel": "Linux 6.8.0-45-generic",
  "uptime": 360000.12,
  "load": [
   0.52,
   0.61,
   0.7
  ],
  "cpus": 32,
  "cpuTicks": {
   "busy": 1560,
   "total": 9760
  },
  "cpuPercent": 37.0,
  "memory": {
   "total": 67108864000,
   "used": 26148864000
  },
  "swap": {
   "total": 8589930496,
   "used": 397930496
  },
  "swapFiles": [
   {
    "path": "/dev/nvme0n1p3",
    "kind": "partition",
    "size": 4294963200,
    "used": 397930496,
    "temporary": false
   },
   {
    "path": "/shellder.swap",
    "kind": "file",
    "size": 4294967296,
    "used": 0,
    "temporary": true
   }
  ],
  "disks": [
   {
    "filesystem": "/dev/nvme0n1p2",
    "mount": "/",
    "total": 1005441241088,
    "used": 512000000000,
    "removable": false,
    "drive": null
   },
   {
    "filesystem": "/dev/sda1",
    "mount": "/mnt/data disk",
    "total": 7999841974272,
    "used": 102400000000,
    "removable": true,
    "drive": "/dev/sda"
   },
   {
    "filesystem": "/dev/sdd1",
    "mount": "/run/media/alice/BACKUP",
    "total": 62500372480,
    "used": 10240000000,
    "removable": true,
    "drive": "/dev/sdd"
   }
  ],
  "volumes": [
   {
    "device": "/dev/sdb1",
    "drive": "/dev/sdb",
    "label": "STICK A",
    "fstype": "vfat",
    "size": 31998951424
   },
   {
    "device": "/dev/mapper/luks-1",
    "drive": "/dev/sdb",
    "label": "SAFE",
    "fstype": "ext4",
    "size": 1000000
   }
  ],
  "network": [
   {
    "name": "eth0",
    "addresses": [
     "10.0.0.5/24",
     "2001:db8::5/64"
    ],
    "received": 123456789,
    "sent": 987654321
   },
   {
    "name": "docker0",
    "addresses": [
     "172.17.0.1/16"
    ],
    "received": 0,
    "sent": 4242
   }
  ],
  "gpus": [
   {
    "index": 0,
    "name": "NVIDIA A100-SXM4-40GB",
    "utilization": 87.0,
    "memory": {
     "total": 42949672960,
     "used": 31457280000
    }
   },
   {
    "index": 1,
    "name": "NVIDIA A100-SXM4-40GB",
    "utilization": 0.0,
    "memory": {
     "total": 42949672960,
     "used": 0
    }
   }
  ],
  "tmux": [
   {
    "name": "main",
    "activity": 1699999000.0,
    "attached": 1,
    "windows": [
     {
      "index": 0,
      "name": "zsh",
      "active": false,
      "command": "zsh",
      "path": "/home/alice",
      "panes": 1
     },
     {
      "index": 1,
      "name": "vim",
      "active": true,
      "command": "vim",
      "path": "/home/alice/src",
      "panes": 2
     },
     {
      "index": 2,
      "name": "htop",
      "active": false,
      "command": "htop",
      "path": "/home/alice",
      "panes": 1
     }
    ],
    "path": "/home/alice/src"
   },
   {
    "name": "train",
    "activity": 1699999500.0,
    "attached": 0,
    "windows": [
     {
      "index": 0,
      "name": "python",
      "active": true,
      "command": "python3",
      "path": "/data/train",
      "panes": 1
     }
    ],
    "path": "/data/train"
   }
  ],
  "screen": [
   {
    "id": "12345.main",
    "name": "main",
    "pid": 12345,
    "attached": true,
    "path": "/home/alice/notes"
   },
   {
    "id": "23456.pts-0.gpu-box",
    "name": "pts-0.gpu-box",
    "pid": 23456,
    "attached": false,
    "path": ""
   }
  ],
  "docker": {
   "tool": "docker",
   "list": [
    {
     "id": "0123abcd",
     "name": "web",
     "image": "nginx:1.27",
     "status": "Up 3 days",
     "ports": "0.0.0.0:8080->80/tcp",
     "mounts": [
      "/srv/web/html",
      "/srv/web/conf"
     ]
    },
    {
     "id": "4567efgh",
     "name": "db",
     "image": "postgres:16",
     "status": "Up 3 days (healthy)",
     "ports": "",
     "mounts": []
    }
   ]
  },
  "slurm": [
   {
    "id": "12345",
    "name": "train-llm",
    "state": "RUNNING",
    "time": "1-02:03:04",
    "nodes": 2,
    "reason": "gpu[01-02]",
    "partition": "gpu",
    "path": "/home/alice/llm"
   },
   {
    "id": "12346",
    "name": "eval | nightly",
    "state": "PENDING",
    "time": "0:00",
    "nodes": 1,
    "reason": "(Priority)",
    "partition": "gpu",
    "path": "/home/alice/eval"
   }
  ],
  "pbs": [
   {
    "id": "777.pbs01",
    "name": "train",
    "state": "R",
    "time": "01:02:03",
    "nodes": 2,
    "node": "node01",
    "queue": "gpu",
    "path": "/home/alice/run"
   },
   {
    "id": "778.pbs01",
    "name": "eval",
    "state": "Q",
    "time": "",
    "nodes": 1,
    "node": "",
    "queue": "gpu",
    "path": ""
   }
  ],
  "vnc": [
   {
    "user": "alice",
    "pid": 2222,
    "program": "Xtigervnc",
    "display": 1,
    "port": 5901
   },
   {
    "user": "bob",
    "pid": 3333,
    "program": "x11vnc",
    "display": 0,
    "port": 5910
   },
   {
    "user": "carol",
    "pid": 4444,
    "program": "Xvnc",
    "display": 3,
    "port": 5903
   }
  ],
  "processes": [
   {
    "pid": 4321,
    "user": "alice",
    "cpu": 250.0,
    "memory": 12.3,
    "command": "python3"
   },
   {
    "pid": 1,
    "user": "root",
    "cpu": 0.0,
    "memory": 0.1,
    "command": "systemd"
   },
   {
    "pid": 5555,
    "user": "bob",
    "cpu": 0.5,
    "memory": 30.0,
    "command": "java"
   }
  ],
  "logins": [
   {
    "user": "alice",
    "tty": "pts/0",
    "since": "2026-10-12 00:10",
    "from": "10.0.0.2"
   },
   {
    "user": "bob",
    "tty": "tty1",
    "since": "2026-10-11 09:00",
    "from": null
   },
   {
    "user": "carol",
    "tty": "pts/3",
    "since": "2026-10-12 00:30",
    "from": "tmux(1234).%0"
   }
  ],
  "users": 3,
  "listeners": [
   {
    "port": 22,
    "address": "0.0.0.0",
    "process": "sshd",
    "pid": 1
   },
   {
    "port": 5901,
    "address": "*",
    "process": "",
    "pid": null
   },
   {
    "port": 8888,
    "address": "127.0.0.1",
    "process": "jupyter-lab",
    "pid": 7777
   }
  ],
  "readParts": []
 },
 "log": [
  "2026-11-23 09:12:04 INFO [731] daemon starting",
  "2026-11-23 09:12:05 INFO [731] bastion: master established",
  "2026-11-23 09:12:07 INFO [731] web-1: master established",
  "2026-11-23 09:12:09 PROMPT [731] gpu-box: password asked, answered from the keychain",
  "2026-11-23 09:12:10 PROMPT [731] gpu-box: verification code asked, answered with a TOTP code",
  "2026-11-23 09:12:11 INFO [731] gpu-box: master established",
  "2026-11-23 14:02:40 WARNING [731] db-staging: master exited: Connection closed by 10.0.3.40 port 22",
  "2026-11-23 14:02:40 INFO [731] db-staging: retrying in 20s"
 ],
 "forwards": [
  {
   "direction": "local",
   "local": 8888,
   "remote": 8888
  }
 ],
 "now": 1795683044
};
