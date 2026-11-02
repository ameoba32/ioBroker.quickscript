// Example script for ioBroker.quickscript
// All listeners, cron jobs, and timers imported from 'iobroker.quickscript'
// are automatically cleaned up on hot-reload!

import { getState, setState, on, schedule, log } from 'iobroker.quickscript';

log('01_example.js started');

// 1. Subscribe to state changes:
on('0_userdata.0.test_trigger', async (obj) => {
    log(`Trigger changed to: ${obj.val}`);
    await setState('0_userdata.0.test_response', `Handled at ${new Date().toISOString()}`, true);
});

// 2. Schedule cron job:
schedule('*/30 * * * *', () => {
    log('30-minute cron tick');
});
