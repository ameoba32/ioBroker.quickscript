// Example 1: Using native ioBroker adapter API + node-cron directly (Zero overhead)
import { getAdapter, cron } from 'iobroker.quickscript';

const adapter = getAdapter();

adapter.log.info('01_example.js started with native ioBroker API');

// 1. Listen to state changes using native adapter event:
const onStateChange = async (id, state) => {
    if (id === '0_userdata.0.test_trigger' && state && !state.ack) {
        adapter.log.info(`Trigger changed to: ${state.val}`);
        await adapter.setForeignStateAsync('0_userdata.0.test_response', {
            val: `Handled at ${new Date().toISOString()}`,
            ack: true,
        });
    }
};
adapter.on('stateChange', onStateChange);

// 2. Schedule cron job via node-cron:
const job = cron.schedule('*/30 * * * *', () => {
    adapter.log.info('30-minute cron tick');
});

// 3. Register cleanup before hot-reload / stop:
adapter.onCleanup(() => {
    adapter.removeListener('stateChange', onStateChange);
    job.stop();
});
