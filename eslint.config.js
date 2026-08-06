const browserGlobals = {
  clearInterval: "readonly",
  clearTimeout: "readonly",
  console: "readonly",
  document: "readonly",
  navigator: "readonly",
  Notification: "readonly",
  requestAnimationFrame: "readonly",
  setInterval: "readonly",
  setTimeout: "readonly",
  URL: "readonly",
  window: "readonly",
};

export default [
  {
    files: ["src-tauri/dist/**/*.js"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: browserGlobals,
    },
    rules: {
      "no-undef": "error",
      "no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },
];
