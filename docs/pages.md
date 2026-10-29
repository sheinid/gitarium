# GitHub Pages embeds

GitHub Actions renders GIFs. GitHub Pages serves the resulting files. Loading an image does not start a render.

## Request a GIF

1. [Open a render request](https://github.com/sheinid/gitarium/issues/new?template=render.yml) and enter a public GitHub repository.
2. Wait for the request to close with `render-ready`. The workflow usually needs a few minutes after it starts. See [Actions](https://github.com/sheinid/gitarium/actions/workflows/pages.yml) for progress or failures.
3. Paste this into your README, with a lowercase owner and repository:

```md
![Repository growing with Gitarium](https://sheinid.github.io/gitarium/embed/OWNER/REPOSITORY.gif)
```

No installation, workflow, or token is needed in your repository. GitHub sign-in is required to submit a request. GIFs refresh every six hours when the default branch changes. GitHub may delay scheduled jobs, and README image caches can delay visible updates.

A request with `render-failed` stays open and retries on later runs. Check the workflow summary for errors. An existing GIF stays available when a refresh fails. Reopen a completed request to retry it immediately.

## Public render limits

The central queue holds up to 24 repositories. Requests enter in creation order after existing registrations. Each repository must be public and no larger than 100 MB according to GitHub's repository metadata. The renderer reads at most 2,000 commits from a shallow clone and samples up to 120 first-parent observations. Files are limited to 10,000 per filtered snapshot.

GIFs are 960 × 540 at 15 fps for 16 seconds, with 1.2-second growth phases and a two-second final hold. Land settles before plants grow, and the exporter reserves time for both phases. Layouts allow 24 islands and 450 visible plants. Clones time out after 90 seconds and rendering after three minutes. GIFs must be at most 10 MB. Use the local CLI for larger repositories, full history, MP4, or different settings.

The workflow checks out only Gitarium's trusted `main` branch. Target repositories are bare clones. Their build scripts and workflows never run. GIFs are cached in Actions and recoverable from the live Pages deployment. Completed request issues and `registry.json` preserve registrations. The workflow updates request labels and closes successful requests after deployment.

## Run your own Pages instance

Fork Gitarium, enable Issues and Actions, and set **Settings → Pages → Build and deployment → Source → GitHub Actions**. Create the labels `gitarium-render`, `render-ready`, and `render-failed` before using the request form. Replace `pages/repositories.json` with your seed repositories, update the repository links in the issue form, docs, and `src/embed.rs`, then run **Render and publish GIFs** manually.

The workflow obtains its public base URL from `actions/configure-pages`, so project Pages paths and custom domains work. A manual run accepts an optional repository. Pushes to `main`, open render requests, and the six-hour schedule also trigger rendering. Runs are serialized and each run collects pending requests.

The default CLI prints URLs for the public instance. For your own instance:

```sh
gitarium embed OWNER/REPOSITORY --base-url https://YOUR-ACCOUNT.github.io/gitarium
```

The JSON result includes both `url` and `request_url`. Change `src/embed.rs` when distributing a CLI that should send requests to your fork.
