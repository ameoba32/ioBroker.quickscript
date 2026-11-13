import config from '@iobroker/eslint-config';

export default [
    ...config,
    {
        ignores: ['bin/**', 'node_modules/**', 'rust/**'],
    },
];
