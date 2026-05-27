import { readFileSync } from "node:fs";

const workflowPath = ".github/workflows/build-desktop.yml";
const workflow = readFileSync(workflowPath, "utf8");

const checks = [
  ["runs macOS ARM build", /macos-14/.test(workflow) && /aarch64-apple-darwin/.test(workflow)],
  ["runs Windows x64 build", /windows-latest/.test(workflow) && /x86_64-pc-windows-msvc/.test(workflow)],
  ["uses pnpm 10", /pnpm\/action-setup@v\d/.test(workflow) && /version:\s*10\.0\.0/.test(workflow)],
  ["sets up Rust stable", /dtolnay\/rust-toolchain@stable/.test(workflow)],
  ["builds with Tauri action", /tauri-apps\/tauri-action@v\d/.test(workflow)],
  ["uploads artifacts", /actions\/upload-artifact@v\d/.test(workflow)],
  ["keeps debug artifacts", /target\/release\/bundle/.test(workflow)],
];

const failed = checks.filter(([, passed]) => !passed);

if (failed.length > 0) {
  console.error(`Invalid ${workflowPath}:`);
  for (const [name] of failed) {
    console.error(`- ${name}`);
  }
  process.exit(1);
}
