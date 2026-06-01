I am primarily using the Tauri app for both MacOS and Windows now, since I use both platforms.
There are tests in the repo, when you add, change, or remove features, also update the tests, to add or remove tests.
When you are finished adding or modifying a feature, always run the tests (including any you have added, etc after adding or modifying the feature), and if any of them come back as not passing, then fix them, until all the tests pass.
Once you make changes, make sure to restart the dev app (for smaller UI changes that Vite can hot reload, then don't worry about it, but for anything slightly larger, etc then do a whole reload, with killing the old app, any old processes, etc)
