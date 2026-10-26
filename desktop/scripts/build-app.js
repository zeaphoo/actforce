/**
 * Build script for creating app-specific distributables.
 *
 * Usage:
 *   node scripts/build-app.js --config=actforce
 *   node scripts/build-app.js --config=actforce --linux --win --mac
 */

const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const CONFIGS_DIR = path.join(ROOT, "configs");

// Parse CLI args
function parseArgs() {
  const args = {};
  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith("--")) {
      const [key, value] = arg.slice(2).split("=");
      args[key] = value || true;
    }
  }
  return args;
}

function main() {
  const args = parseArgs();

  if (!args.config) {
    console.error("Error: --config=<app-id> is required");
    console.error("Available configs:", listConfigs());
    process.exit(1);
  }

  const configPath = path.join(CONFIGS_DIR, `${args.config}.json`);
  if (!fs.existsSync(configPath)) {
    console.error(`Error: config file not found: ${configPath}`);
    console.error("Available configs:", listConfigs());
    process.exit(1);
  }

  const config = JSON.parse(fs.readFileSync(configPath, "utf-8"));
  console.log(`Building "${config.productName}" from config: ${args.config}`);

  // Determine platforms
  const platforms = [];
  if (args.linux) platforms.push("linux");
  if (args.win) platforms.push("win");
  if (args.mac) platforms.push("mac");
  if (platforms.length === 0) {
    // Default: current platform
    platforms.push(process.platform === "darwin" ? "mac" : process.platform === "win32" ? "win" : "linux");
  }

  // Build electron-builder CLI args
  const builderArgs = [
    `--config.appName=${config.name}`,
    `--config.productName=${config.productName}`,
    `--config.directories.output=dist/${config.id}`,
  ];

  // Icon path (if exists)
  const iconDir = path.join(ROOT, "icons", config.id);
  if (fs.existsSync(iconDir)) {
    builderArgs.push(`--config.directories.buildResources=${iconDir}`);
  }

  for (const plat of platforms) {
    console.log(`\n--- Building for ${plat}: ${config.productName} ---`);
    try {
      execSync(`pnpm exec electron-builder --${plat} ${builderArgs.join(" ")}`, {
        cwd: ROOT,
        stdio: "inherit",
      });
    } catch (err) {
      console.error(`Failed to build for ${plat}`);
      process.exit(1);
    }
  }

  console.log(`\nBuild complete! Output in dist/${config.id}/`);
}

function listConfigs() {
  if (!fs.existsSync(CONFIGS_DIR)) return [];
  return fs.readdirSync(CONFIGS_DIR)
    .filter(f => f.endsWith(".json"))
    .map(f => f.replace(".json", ""));
}

main();
