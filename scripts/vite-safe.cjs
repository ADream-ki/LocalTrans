const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");

const nativeRealpath = fs.realpathSync.native?.bind(fs.realpathSync);
if (nativeRealpath) {
  fs.realpathSync.native = (targetPath, ...rest) => {
    const resolvedTarget = path.resolve(targetPath);
    const resolvedCwd = path.resolve("./");
    if (resolvedTarget === resolvedCwd) {
      const err = new Error("EISDIR: illegal operation on a directory");
      err.code = "EISDIR";
      throw err;
    }
    return nativeRealpath(targetPath, ...rest);
  };
}

if (!process.argv.includes("--configLoader")) {
  process.argv.push("--configLoader", "runner");
}

const vitePkgPath = require.resolve("vite/package.json");
const viteBinPath = path.join(path.dirname(vitePkgPath), "bin", "vite.js");
import(pathToFileURL(viteBinPath).href);
