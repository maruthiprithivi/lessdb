# Secrets with SOPS + age

Credentials (Cloudflare API tokens, npm publish tokens, R2 keys) are
stored in the repo **encrypted** with [SOPS](https://github.com/getsops/sops)
and [age](https://age-encryption.org/). Plaintext never touches git.

## One-time setup (each person/machine)

```sh
# macOS
brew install sops age

# generate your key — back this file up; losing it loses access
age-keygen -o ~/.config/sops/age/keys.txt
```

SOPS on macOS also reads `~/Library/Application Support/sops/age/keys.txt`;
symlink if you prefer the XDG path:

```sh
mkdir -p "$HOME/Library/Application Support/sops/age"
ln -s ~/.config/sops/age/keys.txt "$HOME/Library/Application Support/sops/age/keys.txt"
```

## Granting access to a teammate

1. Ask them for their **public** age key (`age-keygen -y keys.txt`).
2. Add it to the `age:` list for the rule in `.sops.yaml`.
3. Re-encrypt the files so their key is a recipient:

```sh
sops updatekeys secrets/*.enc.yaml
```

## Editing secrets

```sh
sops secrets/example.enc.yaml        # opens $EDITOR decrypted; re-encrypts on save
sops -d secrets/example.enc.yaml     # print decrypted (feed into scripts)
```

File layout convention:

```yaml
cloudflare:
  account_id: "..."
  api_token: "..."        # Workers/Pages deploy from CI
npm:
  registry_token: "..."   # publish token from npmjs.com
r2:
  access_key_id: "..."    # optional; only for R2-admin scripting
  secret_access_key: "..."
```

Create a new file: `sops secrets/production.enc.yaml` (the `.enc.` name
matches the `.sops.yaml` creation rule and is the only thing `.gitignore`
allows in `secrets/`).

## Decrypting in CI

GitHub Actions decrypts with the `SOPS_AGE_KEY` repository secret (the
**private** key content from `keys.txt`):

```yaml
- name: Decrypt secrets
  env:
    SOPS_AGE_KEY: ${{ secrets.SOPS_AGE_KEY }}
  run: sops -d secrets/production.enc.yaml > .secrets.yaml
```

Keep the decrypted output out of artifacts; prefer piping single values
into environment variables instead of writing files when possible.

## Rotating keys

```sh
age-keygen -o ~/.config/sops/age/keys.new.txt      # new key
# swap the public key in .sops.yaml, then:
sops updatekeys secrets/*.enc.yaml                 # re-encrypt to all listed keys
# remove the old key from .sops.yaml and updatekeys again to drop it
```

Never commit `keys.txt` or any plaintext secret — `git status` should
only ever show `*.enc.*` files under `secrets/`.
