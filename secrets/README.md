# Secrets (SOPS + age)

Credentials live here as SOPS-encrypted files. Only `*.enc.*` files are
committed; plaintext never is.

```sh
# one-time key setup (per person/machine that needs access)
brew install sops age
age-keygen -o ~/.config/sops/age/keys.txt          # back this up!

# edit / create
sops secrets/production.enc.yaml                   # opens $EDITOR, re-encrypts on save
sops -d secrets/production.enc.yaml > /tmp/plain   # decrypt to stdout (for scripts)

# grant a teammate: add their age PUBLIC key to .sops.yaml creation_rules,
# then re-encrypt:  sops updatekeys secrets/*.enc.yaml
```

CI decrypts with the `SOPS_AGE_KEY` secret (the *private* key exported
from `keys.txt`). Full runbook: website/content/playbooks/secrets-sops.md.
