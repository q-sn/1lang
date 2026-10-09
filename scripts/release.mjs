// Bump the version everywhere, commit, tag and push:
//   pnpm release 0.2.0
// The pushed tag triggers .github/workflows/release.yml.
import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('Usage: pnpm release <major.minor.patch>');
  process.exit(1);
}

const run = (cmd) => execSync(cmd, { stdio: 'inherit' });

if (execSync('git status --porcelain').toString().trim()) {
  console.error('Commit or stash your changes first.');
  process.exit(1);
}

const replace = (file, re, value) => {
  const text = readFileSync(file, 'utf8');
  if (!re.test(text)) throw new Error(`version not found in ${file}`);
  writeFileSync(file, text.replace(re, value));
};

replace('package.json', /"version": "[^"]+"/, `"version": "${version}"`);
replace('src-tauri/tauri.conf.json', /"version": "[^"]+"/, `"version": "${version}"`);
replace('Cargo.toml', /(\[workspace\.package\]\r?\nversion = )"[^"]+"/, `$1"${version}"`);

run('cargo update --workspace');
run('git add package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock');
run(`git commit -m "Release v${version}"`);
run(`git tag v${version}`);
run('git push');
run(`git push origin v${version}`);
console.log(`\nv${version} pushed: the Release workflow will publish it.`);
