const path = require('node:path');
const { tests } = require('@iobroker/testing');

// Integration tests: install the adapter and start it in a test harness.
// See https://github.com/ioBroker/testing for details.
tests.integration(path.join(__dirname, '..'));
