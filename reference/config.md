# Reference: config

`mimus.toml` — one per checked repo. Secrets resolve via env
var -> OS keyring -> `~/.config/mimus/env`; never this file.

```toml
[sources]
docs = ["README.md", "docs/**/*.md", "AGENTS.md", "project/**/*.md"]
comments = true                  # claims inside code comments
cards = ".kuda/modules/*.yaml"   # audit the memory layer too

[check]
mechanical = true                # paths, commands, symbols, counts
semantic = false                 # prose-vs-behavior; advisory only
# kuda_index = ".kuda/index.sqlite"

[rules]
dir = "rules"
# strict = true                  # rule violations also gate

[report]
format = "human"                 # human | json
# min_confidence = "high"
```

Full annotated example: `examples/mimus.toml.example`.
