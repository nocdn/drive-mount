# Drive Mount for iOS

Native iOS app and File Provider extension for exposing cloud connections in Files.

The app registers one File Provider domain per enabled connection. The extension reads the shared App Group connection store and is launched by iOS when Files needs to enumerate or fetch items, so the containing app does not need to stay open.

Current provider state:

- Backblaze B2: lists buckets/files and downloads files using the B2 Native API.
- Google Drive: lists/downloads via Drive API when an OAuth access token is supplied.
- OneDrive: lists/downloads via Microsoft Graph when an access token is supplied.
- Seedbox: settings are present, but it is not registered in Files until a production native FTP/FTPS transport is implemented.

Deployment target is iOS 26. The project is built with the installed iOS 27 SDK.
