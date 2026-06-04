I am primarily using the Tauri app for both MacOS and Windows now, since I use both platforms.
There are tests in the repo, when you add, change, or remove features, also update the tests, to add or remove tests.
When you are finished adding or modifying a feature, always run the tests (including any you have added, etc after adding or modifying the feature), and if any of them come back as not passing, then fix them, until all the tests pass.
Once you make changes, make sure to restart the dev app (for smaller UI changes that Vite can hot reload, then don't worry about it, but for anything slightly larger, etc then do a whole reload, with killing the old app, any old processes, etc)

On MacOS only:
all of the mounted cloud storages should be in the ~/Drives/ folder, as the mount points (mount target in the code), the google drive one should ALWAYS be called 'google-drive' with nothing else in the filename, the Seedbox one should always be called 'seedbox', with nothing else in the filename, and the b2 ones should always be called '[name of the bucket]' and nothing else, so for example I have a bucket that is called 'nocdn-main', so the folder should be titled just 'nocdn-main'.
