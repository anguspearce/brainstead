// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Lint rules for the window's TypeScript: `make lint` runs this, and Prettier handles layout.

import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default tseslint.config(
  { ignores: ["dist/", "src-tauri/", "node_modules/", "_sift/", "design/", "tools/", "extensions/", ".demo/", ".claude/worktrees/"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // Resetting a page's own state when what it shows changes (a test result when a field is
      // edited, a list while the next one loads) is deliberate here, and valid React.
      "react-hooks/set-state-in-effect": "off",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
      // An effect written `() => el.scrollTo(…)` returns the call's result, and React calls what an
      // effect returns as its clean-up. In WebView2 (Windows) `scrollTo` returns a promise, so the
      // clean-up throws and the page goes blank. Braces, or `return` where a clean-up is meant.
      "no-restricted-syntax": [
        "error",
        {
          selector:
            "CallExpression[callee.name=/^use(Layout|Insertion)?Effect$/] > ArrowFunctionExpression.arguments:first-child[body.type=/^(CallExpression|ChainExpression|AwaitExpression|ConditionalExpression|LogicalExpression)$/]",
          message:
            "An effect's arrow body is returned and run as its clean-up: put it in braces (or write `return` if a clean-up is meant).",
        },
      ],
    },
  },
  // The product page's script runs in the browser.
  { files: ["site/**/*.js"], languageOptions: { globals: globals.browser } },
);
