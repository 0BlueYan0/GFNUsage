# [0.4.0](https://github.com/0BlueYan0/GFNUsage/compare/v0.3.0...v0.4.0) (2026-09-26)


### Bug Fixes

* **update:** install updates on macOS when the app runs from a translocated copy ([e0ced65](https://github.com/0BlueYan0/GFNUsage/commit/e0ced656562340f1e1558d72af9dd03580b92d52))


### Features

* **update:** show download progress on the update button ([ce695d6](https://github.com/0BlueYan0/GFNUsage/commit/ce695d61f1c6d8aa5989e8ef58ffd7f7004f2dda))

# [0.3.0](https://github.com/0BlueYan0/GFNUsage/compare/v0.2.0...v0.3.0) (2026-09-26)


### Bug Fixes

* **app:** keep the Dock icon from appearing at launch on macOS ([42f3908](https://github.com/0BlueYan0/GFNUsage/commit/42f3908a49b19c62d831aafc32706083882fda14))
* **app:** let macOS open the app after it is downloaded ([680799a](https://github.com/0BlueYan0/GFNUsage/commit/680799ad9686c559d92e8eef761280853a8d4f88))
* **panel:** a second tray click on Windows brings the panel to the front instead of hiding it ([17084a2](https://github.com/0BlueYan0/GFNUsage/commit/17084a2a00e0d4dc5f8416be2ed68f39f19d9540))
* **panel:** open the panel on the screen that holds the menu bar icon on macOS ([3e26805](https://github.com/0BlueYan0/GFNUsage/commit/3e268058b4f82faedb16809981e964ede5d87c2b))
* **panel:** open the panel under the menu bar icon on first launch on macOS ([a58c5bb](https://github.com/0BlueYan0/GFNUsage/commit/a58c5bb9a360942093b1d4e5d82fc3c8f0a86565))
* **panel:** open the panel under the menu bar icon on macOS and close it with a second click ([4d4882a](https://github.com/0BlueYan0/GFNUsage/commit/4d4882a7e149df18c6524ceab88068e5bac62383))
* **tray:** keep the menu bar item readable on a light menu bar when there is no data ([d84578d](https://github.com/0BlueYan0/GFNUsage/commit/d84578dda9c30a2dfb7476a582ae3a17fdf6b0bd))


### Features

* **app:** keep the macOS app out of the Dock except while signing in ([96dd1e5](https://github.com/0BlueYan0/GFNUsage/commit/96dd1e5a560f4c4e06fccec342374a7986ef9b8f))
* **tray:** show the time left and a progress bar in the macOS menu bar ([4eb3c0d](https://github.com/0BlueYan0/GFNUsage/commit/4eb3c0d7b2d5119825fc52c77de504ea2e07c565))

# [0.2.0](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.3...v0.2.0) (2026-09-23)


### Bug Fixes

* **app:** fetch once at startup even when the refresh interval is off ([1ca6066](https://github.com/0BlueYan0/GFNUsage/commit/1ca6066d94c727509bb84560a67d76c0fa0091df))
* **ui:** name the refresh interval setting after the data, not the app ([44c22e0](https://github.com/0BlueYan0/GFNUsage/commit/44c22e0a9d57446a3086f6513d56f41e338d017f))
* **ui:** redraw the chart from the last good play history, and clear it on logout ([aa8d879](https://github.com/0BlueYan0/GFNUsage/commit/aa8d879291cad2b562f11e41355ef4c2924b3825))
* **ui:** stop a 1px overflow from putting a scrollbar on the main panel ([7595dc0](https://github.com/0BlueYan0/GFNUsage/commit/7595dc08c0eb355a5ed71a60eec16b51be178437))
* **ui:** stop the progress bar from being squashed to nothing ([b32bff4](https://github.com/0BlueYan0/GFNUsage/commit/b32bff465cdd0a8cc99db1dbe18eeb2fd9d8411b))


### Features

* **pace:** project from the last seven days, not the whole period ([4b66715](https://github.com/0BlueYan0/GFNUsage/commit/4b667155e4e623ad5c2eefa11b75402d17fce527))
* **pace:** turn red when the forecast overshoots too ([26493f0](https://github.com/0BlueYan0/GFNUsage/commit/26493f0dd3153bf24e46cba91d3b2a2c7fced218))
* **ui:** chart the period on the main panel ([75dd9ef](https://github.com/0BlueYan0/GFNUsage/commit/75dd9efd61266d7235c93007933d3efbe7b5a5d5))
* **ui:** choose how often the data refreshes, and wire the window watcher in ([9e2723d](https://github.com/0BlueYan0/GFNUsage/commit/9e2723de0f7b29e4a47a298523dd0a722a873d10))
* **ui:** label the trend at today and at the end of the period ([b0fa976](https://github.com/0BlueYan0/GFNUsage/commit/b0fa976eaeb5d9afaaf54ac8570488fb0c40ab6f))
* **ui:** let the pace mark stand proud of the bar ([0c64f0f](https://github.com/0BlueYan0/GFNUsage/commit/0c64f0f9747339b3a7908bd2bc5d591c0bfadd6f))
* **ui:** mark the pace threshold on the progress bar ([2b72b0b](https://github.com/0BlueYan0/GFNUsage/commit/2b72b0b12345e4f4612726802b18a11aed358711))
* **ui:** move the forecast into the chart's tooltip ([8a1e80b](https://github.com/0BlueYan0/GFNUsage/commit/8a1e80b95269ea0c40d419820232571c75b03fc6))
* **ui:** put a percentage scale on the trend chart ([7f234ce](https://github.com/0BlueYan0/GFNUsage/commit/7f234ceee0259a7e2545314e5f3bcecf733f7ed0))
* **watcher:** read what the GeForce NOW window is doing ([522c4af](https://github.com/0BlueYan0/GFNUsage/commit/522c4af4af492f2ce060cdb482f7cc749e96f77b))
* **widget:** show the remaining or used time on the Windows taskbar ([d9857a2](https://github.com/0BlueYan0/GFNUsage/commit/d9857a25f8817e5739439f704423a6d96e5732b7))

## [0.1.3](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.2...v0.1.3) (2026-09-21)


### Bug Fixes

* **app:** let the panel window listen for events ([35093ec](https://github.com/0BlueYan0/GFNUsage/commit/35093ec8fbf509d5cf9cdabd739918bf9d1f32b1))

## [0.1.2](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.1...v0.1.2) (2026-09-21)


### Bug Fixes

* **app:** time the update check out, and fall back to a direct connection ([10287e2](https://github.com/0BlueYan0/GFNUsage/commit/10287e2acbd6529d9b6b97fd02efdd02c92525d5))
* **ui:** reload the panel when it is shown ([aab0f15](https://github.com/0BlueYan0/GFNUsage/commit/aab0f15f6feafb258a40504054e05bfe9c091a93))
* **ui:** stop the About page from freezing the main panel ([02cd379](https://github.com/0BlueYan0/GFNUsage/commit/02cd3790094eb16ad5f0cef605aa8f1530d19cfb))

## [0.1.1](https://github.com/0BlueYan0/GFNUsage/compare/v0.1.0...v0.1.1) (2026-09-21)


### Bug Fixes

* **app:** report what an update check found, and finish a macOS install ([9f6db8d](https://github.com/0BlueYan0/GFNUsage/commit/9f6db8d418685f043128efe6ae284258ed3a8c48))
* **ui:** line up the banner text with its buttons ([7682fcd](https://github.com/0BlueYan0/GFNUsage/commit/7682fcdebc6acde3116bcf6f45bd4d7663fa38cf))
* **ui:** pin the settings header like the sessions one ([dea5e53](https://github.com/0BlueYan0/GFNUsage/commit/dea5e53bb04905af55e6a6fa0c53a035fffe6fa0))

# [0.1.0](https://github.com/0BlueYan0/GFNUsage/compare/a9a0e42b6e9d52c66f1216ba8b4647ac6864c596...v0.1.0) (2026-09-21)


### Bug Fixes

* **app:** surface a failed update install, and test the dismissal rule ([cf50088](https://github.com/0BlueYan0/GFNUsage/commit/cf5008840a356a5f6a203febf7eb6bd318f0626a))
* **auth:** make the login path diagnosable, interruptible and patient ([86544fa](https://github.com/0BlueYan0/GFNUsage/commit/86544faf7830d13259083c61eef80e2930c54029))
* **auth:** sign-in needs two calls, not one ([1f23c05](https://github.com/0BlueYan0/GFNUsage/commit/1f23c059a84a9d140131e13dc8c742528ba78533))
* drop the cached token when credentials change ([b9157ca](https://github.com/0BlueYan0/GFNUsage/commit/b9157ca9d7b26e5a045706b2bdf27365cba73935))
* harden credential storage and refresh flow after review ([155c906](https://github.com/0BlueYan0/GFNUsage/commit/155c9060226194f431f48686b7bc35018c9e4ef0))
* keep a settings-file error from masking a credential error ([33bae7e](https://github.com/0BlueYan0/GFNUsage/commit/33bae7edce526c5bfeab2f53926a2f9fc6861af1))
* open the panel on the first tray click ([bebf454](https://github.com/0BlueYan0/GFNUsage/commit/bebf454c65ddb177c547541e9e6afb04dfddeade))
* refresh the tray whenever the snapshot changes ([ff7d36d](https://github.com/0BlueYan0/GFNUsage/commit/ff7d36deb5d7ce7c32e65f57ad7234f8bf9682e8))
* stop minting a new token on every start ([c46c1e6](https://github.com/0BlueYan0/GFNUsage/commit/c46c1e628d733524732e0243b96c69f3ce8b3dde))
* surface swallowed errors and fix the settings form after review ([3bb1676](https://github.com/0BlueYan0/GFNUsage/commit/3bb1676ba0b498860e5c06629815833ccefd49f5))
* **ui:** hide the panel on Alt+F4 instead of quitting ([3ce117f](https://github.com/0BlueYan0/GFNUsage/commit/3ce117fc84be19c5ab07000f8f4398073b0d37f4))
* **ui:** let the panel body end where the header does ([d300dca](https://github.com/0BlueYan0/GFNUsage/commit/d300dca9dca39e280b75229e9089f59209dc23db))
* **ui:** pin the date-time separator to an ASCII space ([da78b88](https://github.com/0BlueYan0/GFNUsage/commit/da78b88dd45d774777213114da65e1ed6010fc99))


### Features

* anchor the panel to the tray corner ([6a8940d](https://github.com/0BlueYan0/GFNUsage/commit/6a8940dc53f66a6ed64a3faada37ea49ca91b32e))
* **api:** fetch and parse NVIDIA subscription payload ([60638f9](https://github.com/0BlueYan0/GFNUsage/commit/60638f989322839f0cda0bf5cc6142f818afa80b))
* **api:** read the per-session play history ([4b0c633](https://github.com/0BlueYan0/GFNUsage/commit/4b0c63364c8ff6c2e4136e890621cfb088a402ff))
* **api:** take today's usage from the play history ([dac333b](https://github.com/0BlueYan0/GFNUsage/commit/dac333b2b5a77f3f258f8a6b2ff1ddd16ca76315))
* **app:** check for updates and offer to install them ([4005c26](https://github.com/0BlueYan0/GFNUsage/commit/4005c26a451e000ab4755e8a0dc468cddaf20929))
* **app:** keep a single instance and write diagnostics to a file ([ed9b63a](https://github.com/0BlueYan0/GFNUsage/commit/ed9b63a1d43b914d641b0545cb0fbeffbfe0cdbb))
* **auth:** add OAuth sign-in over a localhost loopback ([c5ab0ae](https://github.com/0BlueYan0/GFNUsage/commit/c5ab0ae4353d2407b148557ad7eb1ed0e91a75f5))
* **auth:** add TokenManager with single-flight refresh and rotation ([a27a7f2](https://github.com/0BlueYan0/GFNUsage/commit/a27a7f22754e65de730457f2036f4efb6db81802))
* **auth:** add TokenStore trait with keyring and memory backends ([266013e](https://github.com/0BlueYan0/GFNUsage/commit/266013e95f831b7db955b2dde3099a5e139cba05))
* **auth:** build the PKCE challenge and authorize URL ([c46d7a9](https://github.com/0BlueYan0/GFNUsage/commit/c46d7a97ae978d753f03c711aeb90fb1a8b0eef1))
* **auth:** decode GFN client session data ([a9a0e42](https://github.com/0BlueYan0/GFNUsage/commit/a9a0e42b6e9d52c66f1216ba8b4647ac6864c596))
* **auth:** exchange an authorization code for credentials ([dcc62bb](https://github.com/0BlueYan0/GFNUsage/commit/dcc62bbfba657c29497ba196815afcdadfb7f355))
* **auth:** read JWT expiry claim ([52b48ee](https://github.com/0BlueYan0/GFNUsage/commit/52b48ee70f8430f448fc173d9a46ff41dd497600))
* **auth:** receive the authorization code on a loopback port ([893cbbb](https://github.com/0BlueYan0/GFNUsage/commit/893cbbb39ae2e4a5d0f686bb0291c7891e290983))
* **auth:** remember when the client token expires ([cf7d36d](https://github.com/0BlueYan0/GFNUsage/commit/cf7d36db69b1de60106edf43adaf5534ca021024))
* **auth:** sign in through a webview, not a loopback port ([883e226](https://github.com/0BlueYan0/GFNUsage/commit/883e226c7f9ac90e04556c1ade117610958a50d7))
* export and import the blackout schedule ([96fe50a](https://github.com/0BlueYan0/GFNUsage/commit/96fe50add9eed46478749557d169a5932d3b5601))
* **pace:** add blackout schedule model and settings store ([9e4cba0](https://github.com/0BlueYan0/GFNUsage/commit/9e4cba06420a1bdbda7a4ca4efc564dfdedb0290))
* **pace:** compute playable minutes from the blackout schedule ([8b2669f](https://github.com/0BlueYan0/GFNUsage/commit/8b2669f65f3213547b1e27f747ced6c31477e206))
* **pace:** derive pace threshold, projection and today's budget ([7d8f3ce](https://github.com/0BlueYan0/GFNUsage/commit/7d8f3ce67b4cfa8aa9fd6bd29e739799cccec4fb))
* **pace:** find when the quota runs out by walking free intervals ([52ebded](https://github.com/0BlueYan0/GFNUsage/commit/52ebded113f2e9a3d0c65a76b7281e034942bd09))
* poll again as soon as the machine wakes up ([6ecf1ed](https://github.com/0BlueYan0/GFNUsage/commit/6ecf1ed35f60430e1855b38675e6efa7fecd866b))
* **quota:** derive display snapshot from subscription ([74a53b1](https://github.com/0BlueYan0/GFNUsage/commit/74a53b185dfa22ec71e06d89f03c2fb46d3acde6))
* recompute pace every poll and expose the schedule commands ([615e707](https://github.com/0BlueYan0/GFNUsage/commit/615e7076f3c015b0adf8ef2c2866a3a3d38b5749))
* **store:** accumulate a snapshot history ([c08f7d4](https://github.com/0BlueYan0/GFNUsage/commit/c08f7d46ebdb5bce0d7ee0c4699358c1172192fd))
* **tray:** render remaining hours into a tray icon ([35a61a3](https://github.com/0BlueYan0/GFNUsage/commit/35a61a3d937bff3c69c8ae42b2fba03396333d29))
* **tray:** show the over-pace state and flag an exhausted quota ([ec0eb1e](https://github.com/0BlueYan0/GFNUsage/commit/ec0eb1e7084c7b342e2e18576e2287162f94a700))
* **ui:** add panel with credential import and quota display ([ec60319](https://github.com/0BlueYan0/GFNUsage/commit/ec603199cc3fca5c0be47c7b35205e1b5fea235e))
* **ui:** add the blackout schedule editor ([59ac834](https://github.com/0BlueYan0/GFNUsage/commit/59ac834949843463efe18c7575cbf8c3781babda))
* **ui:** count down to the hour, not just the day ([641e894](https://github.com/0BlueYan0/GFNUsage/commit/641e894c0975dd6cb3a3696f8bbf1d36e73d9aa8))
* **ui:** give the play history its own page ([87ec1fd](https://github.com/0BlueYan0/GFNUsage/commit/87ec1fd3727dc89021d05000a9eaffa3eb035fb1))
* **ui:** show pace threshold and projection in the panel ([56d6d4e](https://github.com/0BlueYan0/GFNUsage/commit/56d6d4e65271ab44c390618187961cc7c5f064b6))
* **ui:** show the version, updates and autostart on an About page ([9deddee](https://github.com/0BlueYan0/GFNUsage/commit/9deddeeadd72688013491b5482d2a7d5a10734f1))
* **ui:** trim the panel to numbers and fix today's budget ([6330010](https://github.com/0BlueYan0/GFNUsage/commit/63300101f24739e496f601769a35694348b1fb2c))
* **ui:** warn before the credential expires and hint about the tray overflow ([4ad0207](https://github.com/0BlueYan0/GFNUsage/commit/4ad020702e612e9496c53343b39aa145074086b3))
* wire tray icon to subscription polling loop ([3f5464c](https://github.com/0BlueYan0/GFNUsage/commit/3f5464ce24cddf69e13e77ef041830b9786b166d))
