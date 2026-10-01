/* eslint-disable */
// Load the platform-specific napi binary (built with `napi build`, or by
// copying the cargo cdylib to lessdb-node.<platform>-<arch>.node).
function loadNative() {
  const candidates = [
    `./lessdb-node.${process.platform}-${process.arch}.node`,
    "./lessdb-node.node",
    "./index.node",
  ];
  let lastErr;
  for (const c of candidates) {
    try {
      return require(c);
    } catch (e) {
      lastErr = e;
    }
  }
  throw lastErr;
}

const { Connection } = loadNative();

function open(path) {
  return Connection.open(path ?? ".less");
}

module.exports = { open, Connection };
