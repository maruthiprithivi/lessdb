const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const code = fs.readFileSync('website/github-stars.js', 'utf8');
async function run({ cache = null, response, failure = false, storageFailure = false } = {}) {
  const nodes = [0, 1].map(() => ({textContent: '', attrs: {}, setAttribute(k, v) {this.attrs[k] = v;}}));
  let calls = 0;
  let stored;
  vm.runInNewContext(code, {
    document: {querySelectorAll: () => nodes}, Date, Number, JSON, AbortController,
    setTimeout, clearTimeout,
    localStorage: {
      getItem() { if (storageFailure) throw Error(); return cache; },
      setItem(k, v) { if (storageFailure) throw Error(); stored = JSON.parse(v); },
    },
    fetch: async (url, options) => {
      calls++;
      assert.equal(url, 'https://api.github.com/repos/lessdb/lessdb');
      assert.equal(options.credentials, 'omit');
      if (failure) throw Error('network');
      return {ok: response?.ok !== false, json: async () => response};
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  return {nodes, calls, stored};
}
const repo = {private: false, full_name: 'lessdb/lessdb', stargazers_count: 1234};
test('both desktop/mobile counters use exact live count and cache', async () => {
  const r = await run({response: repo});
  assert.equal(r.calls, 1); assert.equal(r.stored.count, 1234);
  for (const node of r.nodes) assert.equal(node.textContent, '★ 1,234');
});
test('fresh cached count avoids network and labels cache', async () => {
  const r = await run({cache: JSON.stringify({count: 0, at: Date.now()})});
  assert.equal(r.calls, 0); assert.equal(r.nodes[0].textContent, '★ 0');
  assert.match(r.nodes[0].attrs['aria-label'], /cached/);
});
test('stale and corrupt cache refetch', async () => {
  for (const cache of ['oops', JSON.stringify({count: 5, at: Date.now() - 3600001})]) {
    assert.equal((await run({cache, response: repo})).calls, 1);
  }
});
test('network/rate limit/private/invalid responses hide count, never invent zero', async () => {
  for (const options of [{failure:true}, {response:{ok:false}}, {response:{...repo,private:true}}, {response:{...repo,stargazers_count:-1}}, {response:{...repo,stargazers_count:'12'}}, {response:{...repo,full_name:'other/repo'}}]) {
    const r = await run(options);
    assert.equal(r.nodes[0].textContent, '');
    assert.match(r.nodes[0].attrs['aria-label'], /unavailable/);
  }
});
test('storage disabled still fetches and renders', async () => {
  assert.equal((await run({storageFailure:true,response:repo})).nodes[0].textContent, '★ 1,234');
});
test('home has keyboard-accessible links in desktop and mobile navigation', () => {
  const html = fs.readFileSync('website/index.html','utf8');
  assert.equal((html.match(/class="github-link"/g)||[]).length,2);
  assert.equal((html.match(/href="https:\/\/github.com\/lessdb\/lessdb" class="github-link"/g)||[]).length,2);
});
