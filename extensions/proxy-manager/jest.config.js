/** @type {import('ts-jest').JestConfigWithTsJest} */
module.exports = {
  testEnvironment: 'node',
  roots: ['<rootDir>/src'],
  testMatch: ['**/__tests__/**/*.test.ts'],
  clearMocks: true,
  // Source ships as ESM with explicit ".js" import specifiers (browser-loadable
  // as type="module"). For the Node test runner, ts-jest transpiles to CommonJS
  // and we strip the ".js" so it resolves the ".ts" source.
  moduleNameMapper: {
    '^(\\.{1,2}/.*)\\.js$': '$1'
  },
  transform: {
    '^.+\\.ts$': [
      'ts-jest',
      {
        tsconfig: {
          module: 'commonjs',
          moduleResolution: 'node',
          noUnusedLocals: false,
          noUnusedParameters: false
        }
      }
    ]
  }
};
