/**
 * Codync API credential: the Codync host on this computer.
 *
 * The Codync host starts n8n with CODYNC_HOST_URL and CODYNC_HOST_TOKEN in its
 * environment, so a workflow works with an empty credential. The two fields
 * exist to point the node at another host (or to read back what the host set).
 */

class CodyncApi {
    name = 'codyncApi';
    displayName = 'Codync API';
    description = 'The Codync host on this computer. Leave it empty: the host fills it in.';
    properties = [
        {
            displayName: 'Host URL',
            name: 'hostUrl',
            type: 'string',
            default: '',
            placeholder: 'http://127.0.0.1:19222',
            description: 'Leave empty to use the one the Codync host sets up',
        },
        {
            displayName: 'Token',
            name: 'token',
            type: 'string',
            default: '',
            typeOptions: { password: true },
            description: 'Leave empty to use the one the Codync host sets up',
        },
    ];
}

module.exports = { CodyncApi };
