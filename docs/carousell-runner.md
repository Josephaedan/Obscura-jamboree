# Running Carousell searches on your own computer

Carousell blocks GitHub's servers, so the **Carousell search** workflow runs
its searches on your computer through a GitHub *self-hosted runner*. GitHub
still builds the `obscura` binary; your computer only downloads it and runs
the searches, so you don't need Rust or any build tools.

Works on a Mac or Linux computer. On Windows, set the runner up inside WSL
(Ubuntu) and follow the Linux steps.

## 1. Block strangers from using your computer (do this first)

The repository is public, so someone could open a pull request that tries to
run code on your runner. Require approval before any outside pull request
runs a workflow:

**Repo → Settings → Actions → General → "Approval for running fork pull
request workflows from contributors" → Require approval for all external
contributors → Save.**

Never approve workflow runs on pull requests from people you don't know.

## 2. Install the runner

1. On GitHub: **Repo → Settings → Actions → Runners → New self-hosted
   runner**, then choose **macOS** or **Linux** and your computer's
   architecture (Apple Silicon Macs are ARM64).
2. Open a terminal on your computer (as your normal user, not root) and run
   the **Download** commands GitHub shows.
3. Run the **Configure** command GitHub shows, adding `--labels carousell`
   to the end, for example:

   ```bash
   ./config.sh --url https://github.com/<you>/Obscura-jamboree --token <TOKEN> --labels carousell
   ```

   Press Enter to accept the other defaults.

## 3. Start it automatically

Still in the runner folder:

```bash
# macOS
./svc.sh install && ./svc.sh start

# Linux
sudo ./svc.sh install && sudo ./svc.sh start
```

The runner now starts on boot. Check **Settings → Actions → Runners**: it
should show as **Idle**.

## 4. Check Python

The search script needs Python 3. Run `python3 --version`. On a Mac, if it
offers to install the Command Line Tools, accept.

## 5. Keep the computer awake

Runs wait in the queue until your computer is on and the runner is online
(GitHub cancels a run after 24 hours of waiting). On a Mac, stop it from
sleeping while plugged in: **System Settings → Battery/Energy → Prevent
automatic sleeping when the display is off.**

## Using it

- Edit [`carousell-searches.json`](../carousell-searches.json) and merge to
  `main`; the searches run automatically.
- Results appear on the run's summary page (**Actions → Carousell search →**
  latest run) and as a downloadable `carousell-listings-*` artifact.
- To remove the runner later: `./svc.sh uninstall`, then
  `./config.sh remove --token <TOKEN>` (GitHub shows the token when you
  remove the runner in Settings).
