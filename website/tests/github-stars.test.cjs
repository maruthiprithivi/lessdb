const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');

test('public website exposes no personal repository identity', () => {
  const paths = [
    'website/index.html',
    'website/github-stars.js',
    'website/content/playbooks/website-deploy.md',
    'website/content/playbooks/manual-cloudflare-tasks.md',
  ];
  for (const path of paths) {
    const text = fs.readFileSync(path, 'utf8');
    assert.doesNotMatch(text, /@gmail\.com/);
  }
});

test('project metrics script performs no external identity lookup', () => {
  const text = fs.readFileSync('website/github-stars.js', 'utf8');
  assert.doesNotMatch(text, /api\.github\.com|fetch\(/);
});
