"""Render public GitHub repositories as static GIFs for GitHub Pages.

Repository contents are Git data only. No checkout, build, or repository script runs.
"""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from datetime import datetime, timezone
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode, urlsplit
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / 'work/pages-cache'
RESULTS = ROOT / 'work/pages-results.json'
MAX_REPOS = 24
MAX_GIF_BYTES = 10 * 1024 * 1024
MAX_REPO_KB = 100_000
REQUEST_LABEL = 'gitarium-render'
DEFAULT_BASE = 'https://sheinid.github.io/gitarium'


def repository(value):
    raw = value.strip().removeprefix('https://github.com/').rstrip('/')
    if raw.endswith('.git'):
        raw = raw[:-4]
    parts = raw.split('/')
    if len(parts) != 2:
        raise ValueError('Use OWNER/REPOSITORY or an https://github.com URL')
    owner, name = parts
    if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?', owner):
        raise ValueError('Invalid GitHub owner')
    if not re.fullmatch(r'[A-Za-z0-9_.-]{1,100}', name) or name in ('.', '..'):
        raise ValueError('Invalid GitHub repository name')
    return f'{owner}/{name}'.lower()


def issue_repository(issue):
    match = re.search(r'^### Repository\s*\n+([^\n]+)', issue.get('body') or '', re.M)
    if not match:
        raise ValueError('Missing Repository field. Use the render request form.')
    return repository(match[1])


def api(path, method='GET', payload=None):
    if not path.startswith('/repos/'):
        raise ValueError('Expected a repository API path')
    headers = {'Accept': 'application/vnd.github+json', 'User-Agent': 'Gitarium-Pages'}
    token = os.environ.get('GH_TOKEN')
    if token:
        headers['Authorization'] = f'Bearer {token}'
    data = None if payload is None else json.dumps(payload).encode()
    if data is not None:
        headers['Content-Type'] = 'application/json'
    with urlopen(Request('https://api.github.com' + path, data=data, headers=headers,
                         method=method), timeout=25) as response:
        body = response.read()
        return json.loads(body) if body else None


def request_issues(host):
    issues = []
    for page in range(1, 11):
        batch = api(f'/repos/{host}/issues?state=all&labels={REQUEST_LABEL}'
                    f'&sort=created&direction=asc&per_page=100&page={page}')
        for issue in batch:
            labels = {label['name'] for label in issue.get('labels', [])}
            if 'pull_request' not in issue and (issue['state'] == 'open' or 'render-ready' in labels):
                issues.append(issue)
        if len(batch) < 100:
            break
    return issues


def gif_path(cache, slug):
    return cache / 'embed' / (repository(slug) + '.gif')


def valid_gif(path):
    if not path.is_file() or not 10 <= path.stat().st_size <= MAX_GIF_BYTES:
        return False
    with path.open('rb') as stream:
        return stream.read(6) in (b'GIF87a', b'GIF89a')


def load_registry(cache):
    try:
        entries = json.loads((cache / 'registry.json').read_text())
        return {repository(item['repository']): item for item in entries[:MAX_REPOS]}
    except (OSError, ValueError, TypeError, KeyError):
        return {}


def download(url, path, limit):
    with urlopen(Request(url, headers={'User-Agent': 'Gitarium-Pages'}), timeout=25) as response:
        body = response.read(limit + 1)
    if len(body) > limit:
        raise ValueError('Published asset exceeds the size limit')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)


def restore_published(cache, base):
    """Recover after Actions evicts its cache. Pages remains the published backup."""
    cache.mkdir(parents=True, exist_ok=True)
    registry = load_registry(cache)
    try:
        if not registry:
            download(base + '/registry.json', cache / 'registry.json', 128 * 1024)
            registry = load_registry(cache)
        for slug in registry:
            target = gif_path(cache, slug)
            if not valid_gif(target):
                download(f'{base}/embed/{slug}.gif', target, MAX_GIF_BYTES)
    except (OSError, ValueError, URLError) as error:
        print('Published cache unavailable: ' + json.dumps(str(error)))
    return registry


def renderer_version():
    digest = hashlib.sha256()
    sources = sorted((ROOT / 'src').rglob('*.rs')) + sorted((ROOT / 'assets').glob('*.png'))
    for path in sources + [ROOT / 'Cargo.lock', Path(__file__)]:
        digest.update(path.read_bytes())
    return digest.hexdigest()[:16]


def public_head(slug):
    metadata = api(f'/repos/{slug}')
    if metadata.get('private') or metadata.get('disabled'):
        raise ValueError('Only public, enabled GitHub repositories can be rendered')
    if metadata.get('size', MAX_REPO_KB + 1) > MAX_REPO_KB:
        raise ValueError('Repository exceeds the 100 MB public render limit. Use the CLI locally.')
    commits = api(f'/repos/{slug}/commits?per_page=1')
    if not commits:
        raise ValueError('Repository has no commits')
    sha = commits[0]['sha']
    if not re.fullmatch(r'[0-9a-f]{40,64}', sha):
        raise ValueError('Invalid GitHub commit ID')
    return sha


def render_gif(slug, sha, output, binary):
    env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null', GIT_TERMINAL_PROMPT='0')
    with tempfile.TemporaryDirectory(prefix='gitarium-') as directory:
        bare = Path(directory) / slug.split('/')[1]
        subprocess.run(['git', '-c', 'credential.helper=', '-c', 'core.hooksPath=/dev/null',
                        '-c', 'protocol.file.allow=never', 'clone', '--bare', '--single-branch',
                        '--depth', '2000', '--', f'https://github.com/{slug}.git', str(bare)],
                       check=True, timeout=90, env=env, capture_output=True)
        # Use the exact revision checked through the API. A concurrent push is retried next run.
        subprocess.run([str(binary), 'export', str(bare), '--revision', sha, '-o', str(output),
                        '--width', '960', '--height', '540', '--duration', '16', '--fps', '15',
                        '--transition', '1.2', '--hold', '2', '--max-commits', '120',
                        '--max-islands', '24', '--max-plants', '450', '--max-files', '10000'],
                       check=True, timeout=180, env=env, capture_output=True)


def update_repository(slug, cache, old, version, binary):
    """Publish a new GIF only after a complete, validated render."""
    sha = public_head(slug)
    target = gif_path(cache, slug)
    if old.get('sha') == sha and old.get('renderer') == version and valid_gif(target):
        return old
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=target.parent) as directory:
        pending = Path(directory) / 'render.gif'
        render_gif(slug, sha, pending, binary)
        if not valid_gif(pending):
            raise ValueError('Rendered GIF is invalid or exceeds 10 MB')
        pending.replace(target)
    return {'repository': slug, 'sha': sha, 'renderer': version,
            'updated_at': datetime.now(timezone.utc).isoformat(timespec='seconds'),
            'bytes': target.stat().st_size}


def write_site(cache, base, entries, host):
    entries = sorted(entries.values(), key=lambda item: item['repository'])
    (cache / 'registry.json').write_text(json.dumps(entries, indent=2) + '\n')
    (cache / '.nojekyll').touch()
    links = '\n'.join(f'<li><a href="embed/{item["repository"]}.gif">{item["repository"]}</a></li>'
                      for item in entries)
    request = f'https://github.com/{host}/issues/new'
    preview = next((item['repository'] for item in entries if item['repository'] == host),
                   entries[0]['repository'])
    page = '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Gitarium</title>
<style>
body{max-width:760px;margin:64px auto;padding:0 24px;background:#fcfcf9;color:#263b33;font:16px/1.6 system-ui,sans-serif}h1{font-size:32px;margin-bottom:8px}h2{font-size:20px;margin-top:40px}a{color:#306b5b}input,button{font:inherit;padding:8px 12px;border:1px solid #aabcb3;border-radius:4px;background:white;color:inherit}input{width:230px}button{cursor:pointer}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#edf3ee;padding:16px;font-size:13px}img{width:100%;image-rendering:pixelated;margin-top:20px}small{color:#52695c}
</style>
<h1>Gitarium</h1>
<p>A Git repository, growing into a pond. Files become plants, directories become islands.</p>
<h2>Put your repository in a README</h2>
<p>Request a GIF once. When the request closes with <code>render-ready</code>, paste the image below into your README. Changed repositories refresh every six hours.</p>
<form action="REQUEST" method="get">
<input type="hidden" name="template" value="render.yml">
<label>Repository <input id="repo" name="repository" placeholder="OWNER/REPOSITORY" required pattern="[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+"></label>
<button>Request a GIF</button>
</form>
<pre id="markdown">![Repository growing with Gitarium](BASE/embed/OWNER/REPOSITORY.gif)</pre>
<small>Public GitHub repositories only. No installation or token is needed in your repository. Rendering may take a few minutes after Actions starts. The public queue holds up to 24 repositories. <a href="SOURCE/blob/main/docs/pages.md">Limits and setup</a>.</small>
<h2>Available timelapses</h2>
<ul>LINKS</ul>
<img src="embed/PREVIEW.gif" alt="Gitarium growing from its own history">
<p><a href="SOURCE">Source and CLI</a></p>
<script>
const input=document.getElementById('repo');
input.addEventListener('input',()=>{const repo=input.value.trim().toLowerCase();if(input.checkValidity())document.getElementById('markdown').textContent='!['+repo+' growing with Gitarium](BASE/embed/'+repo+'.gif)';});
</script></html>
'''
    page = page.replace('REQUEST', html.escape(request, quote=True)).replace('SOURCE', f'https://github.com/{host}')
    page = page.replace('LINKS', links).replace('BASE', base).replace('PREVIEW', preview)
    (cache / 'index.html').write_text(page)


def render():
    host = repository(os.environ.get('GITHUB_REPOSITORY', 'sheinid/gitarium'))
    base = os.environ.get('PAGES_BASE_URL', DEFAULT_BASE).rstrip('/')
    parsed = urlsplit(base)
    if parsed.scheme != 'https' or not re.fullmatch(r'[a-zA-Z0-9./_-]+', parsed.netloc + parsed.path):
        raise ValueError('Invalid Pages base URL')
    event_path = os.environ.get('GITHUB_EVENT_PATH')
    event = json.loads(Path(event_path).read_text()) if event_path else {}
    registry = restore_published(CACHE, base)
    issues = request_issues(host)
    pending = []
    slugs = []
    results = []
    for value in json.loads((ROOT / 'pages/repositories.json').read_text()):
        slug = repository(value)
        if slug not in slugs:
            slugs.append(slug)
    for slug in registry:
        if slug not in slugs:
            slugs.append(slug)
    manual = event.get('inputs', {}).get('repository')
    if manual:
        slug = repository(manual)
        if slug not in slugs:
            slugs.append(slug)
    for issue in issues:
        try:
            slug = issue_repository(issue)
            if slug not in slugs:
                slugs.append(slug)
            if issue['state'] == 'open':
                pending.append((issue['number'], slug))
        except ValueError as error:
            if issue['state'] == 'open':
                results.append({'issue': issue['number'], 'ok': False, 'error': str(error)})
    version = renderer_version()
    entries = {}
    failures = {}
    for index, slug in enumerate(slugs):
        if index >= MAX_REPOS:
            failures[slug] = 'The public render queue is full. Use the CLI locally.'
            continue
        old = registry.get(slug, {})
        try:
            entries[slug] = update_repository(slug, CACHE, old, version, ROOT / 'target/release/gitarium')
            print(f'Ready: {slug}')
        except (OSError, ValueError, URLError, subprocess.SubprocessError) as error:
            failures[slug] = str(error)[:500]
            print(f'Failed {slug}: ' + json.dumps(str(error)))
            if old and valid_gif(gif_path(CACHE, slug)):
                entries[slug] = old
    for number, slug in pending:
        results.append({'issue': number, 'repository': slug, 'ok': slug not in failures,
                        'error': failures.get(slug)})
    RESULTS.parent.mkdir(parents=True, exist_ok=True)
    RESULTS.write_text(json.dumps(results, indent=2) + '\n')
    if not entries:
        raise RuntimeError('No GIFs available. Preserve the previous Pages deployment.')
    # Do not publish orphaned assets that no longer have a registry entry.
    for path in (CACHE / 'embed').rglob('*.gif'):
        if path.relative_to(CACHE / 'embed').as_posix()[:-4] not in entries:
            path.unlink()
    write_site(CACHE, base, entries, host)
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        lines = [f'Published [{slug}]({base}/embed/{slug}.gif)' for slug in sorted(entries)]
        lines += [f'Failed {slug}: {json.dumps(error)}' for slug, error in failures.items()]
        with open(summary, 'a') as stream:
            stream.write('\n\n'.join(lines) + '\n')


def complete():
    host = repository(os.environ.get('GITHUB_REPOSITORY', 'sheinid/gitarium'))
    for result in json.loads(RESULTS.read_text()):
        number = int(result['issue'])
        label = 'render-ready' if result['ok'] else 'render-failed'
        opposite = 'render-failed' if result['ok'] else 'render-ready'
        path = f'/repos/{host}/issues/{number}'
        try:
            api(path + '/labels/' + quote(opposite), 'DELETE')
        except HTTPError as error:
            if error.code != 404:
                raise
        api(path + '/labels', 'POST', {'labels': [label]})
        if result['ok']:
            api(path, 'PATCH', {'state': 'closed', 'state_reason': 'completed'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['render', 'complete'])
    args = parser.parse_args()
    {'render': render, 'complete': complete}[args.command]()
