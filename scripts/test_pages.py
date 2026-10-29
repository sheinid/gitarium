import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import pages


class PagesTests(unittest.TestCase):
    def test_repository_ids_reject_paths_urls_and_commands(self):
        for value in ['../repo', 'owner/..', 'owner/repo/extra', '-owner/repo',
                      'https://evil.example/owner/repo', 'owner/repo?x=y',
                      'owner/repo;cmd', 'owner/repo%2f..', 'owner/repo\ncmd']:
            with self.subTest(value=value), self.assertRaises(ValueError):
                pages.repository(value)
        self.assertEqual(pages.repository('https://github.com/Fractal-Web/tax.ipk.git/'),
                         'fractal-web/tax.ipk')
        self.assertEqual(pages.repository('x/.github'), 'x/.github')

    def test_issue_form_is_parsed_without_using_title_as_code(self):
        issue = {'title': '$(dangerous)', 'body': '### Repository\n\nOwner/repo\n\n### Other\n\ntext'}
        self.assertEqual(pages.issue_repository(issue), 'owner/repo')
        with self.assertRaises(ValueError):
            pages.issue_repository({'body': '### Repository\n\nowner/repo && command'})

    def test_failed_render_preserves_the_previous_gif(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            target = pages.gif_path(cache, 'owner/repo')
            target.parent.mkdir(parents=True)
            target.write_bytes(b'GIF89a' + b'original' * 20)
            original = target.read_bytes()
            with patch.object(pages, 'public_head', return_value='a' * 40), \
                 patch.object(pages, 'render_gif', side_effect=RuntimeError('encoder failed')):
                with self.assertRaises(RuntimeError):
                    pages.update_repository('owner/repo', cache, {}, 'new', Path('/unused'))
            self.assertEqual(target.read_bytes(), original)
            self.assertEqual(list(target.parent.iterdir()), [target])

    def test_unchanged_repositories_do_not_clone_or_render(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            target = pages.gif_path(cache, 'owner/repo')
            target.parent.mkdir(parents=True)
            target.write_bytes(b'GIF89a' + b'x' * 20)
            old = {'repository': 'owner/repo', 'sha': 'a' * 40, 'renderer': 'same'}
            with patch.object(pages, 'public_head', return_value='a' * 40), \
                 patch.object(pages, 'render_gif') as encoder:
                self.assertEqual(pages.update_repository('owner/repo', cache, old, 'same', Path('/unused')), old)
                encoder.assert_not_called()

    def test_no_target_repository_code_is_executed_and_token_is_not_forwarded(self):
        with tempfile.TemporaryDirectory() as directory, \
             patch.dict(pages.os.environ, {'GH_TOKEN': 'test-token'}), \
             patch.object(pages.subprocess, 'run') as run:
            pages.render_gif('owner/repo', 'a' * 40, Path(directory) / 'out.gif', Path('/trusted/gitarium'))
            clone, render = run.call_args_list
            self.assertIn('--bare', clone.args[0])
            self.assertEqual(clone.args[0][-2], 'https://github.com/owner/repo.git')
            self.assertEqual(Path(clone.args[0][-1]).name, 'repo')
            self.assertEqual(render.args[0][0], '/trusted/gitarium')
            for call in run.call_args_list:
                self.assertNotIn('GH_TOKEN', call.kwargs['env'])
                self.assertNotIn('shell', call.kwargs)

    def test_private_or_oversized_repositories_stop_before_commit_lookup(self):
        for metadata in [{'private': True}, {'size': pages.MAX_REPO_KB + 1}]:
            with self.subTest(metadata=metadata), patch.object(pages, 'api', return_value=metadata) as api:
                with self.assertRaises(ValueError):
                    pages.public_head('owner/repo')
                api.assert_called_once()

    def test_queue_keeps_completed_registrations_and_skips_closed_failures_and_prs(self):
        def issue(number, state, labels):
            return {'number': number, 'state': state, 'labels': [{'name': x} for x in labels]}
        batch = [issue(1, 'open', ['gitarium-render']),
                 issue(2, 'closed', ['gitarium-render', 'render-ready']),
                 issue(3, 'closed', ['gitarium-render', 'render-failed']),
                 dict(issue(4, 'open', ['gitarium-render']), pull_request={})]
        with patch.object(pages, 'api', return_value=batch):
            self.assertEqual([x['number'] for x in pages.request_issues('owner/repo')], [1, 2])

    def test_site_registry_and_links_share_the_same_repository_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            entries = {'owner/repo': {'repository': 'owner/repo', 'sha': 'a' * 40}}
            pages.write_site(cache, pages.DEFAULT_BASE, entries, 'sheinid/gitarium')
            self.assertEqual(pages.load_registry(cache), entries)
            self.assertIn('embed/owner/repo.gif', (cache / 'index.html').read_text())
            (cache / 'registry.json').write_text(json.dumps([{'repository': '../../bad'}]))
            self.assertEqual(pages.load_registry(cache), {})


if __name__ == '__main__':
    unittest.main()
