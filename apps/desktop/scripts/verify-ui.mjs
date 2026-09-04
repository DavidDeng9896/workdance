import { access } from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const ui = path.join(root, "..", "ui");
const required = [
  "styles.css",
  "settings.html",
  "permissions.html",
  "calibration.html",
  "common.js",
];

for (const f of required) {
  await access(path.join(ui, f), constants.R_OK);
}
console.log("ui ok:", required.join(", "));
