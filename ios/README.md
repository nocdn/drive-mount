# Drive Mount for iOS

Native iOS app and File Provider extension for exposing cloud connections in Files.

The app registers one File Provider domain per enabled connection. The extension reads the shared App Group connection store and is launched by iOS when Files needs to enumerate or fetch items, so the containing app does not need to stay open.

Current provider state:

- Backblaze B2: one shared key can expose multiple buckets. Each named bucket is listed in the app and registered as its own Files location.
- Google Drive: lists/downloads via Drive API when an OAuth access token is supplied.
- OneDrive: lists/downloads via Microsoft Graph when an access token is supplied.
- Seedbox: lists and streams files over SFTP (offset reads in 256 KiB chunks) so 1–12 GB transfers stay out of process memory. Existing port 21 settings are treated as the old FTPS default and connect on port 22.

Deployment target is iOS 26. The project is built with the installed iOS 27 SDK.
