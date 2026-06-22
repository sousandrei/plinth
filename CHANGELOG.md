# Changelog

## [1.3.0](https://github.com/sousandrei/plinth/compare/plinth-v1.2.1...plinth-v1.3.0) (2026-06-22)


### Features

* add embedding progress on pre training step ([139b399](https://github.com/sousandrei/plinth/commit/139b3991d793ea3e78ec353603eb68bebe1f7258))
* auto refresh pages on sync data ([3a75c19](https://github.com/sousandrei/plinth/commit/3a75c19b8b14e3e050ff02afe1cdb765795908b5))
* better bulk actions ([058c37e](https://github.com/sousandrei/plinth/commit/058c37e7adf6e995fcab5c0923225b6c32f43bd6))
* better file import ux ([bc104f8](https://github.com/sousandrei/plinth/commit/bc104f8e65f09b991aac10bbb282108dd5331bf5))
* better streaming based pairing for large spaces ([18ab290](https://github.com/sousandrei/plinth/commit/18ab2905eb665228c76607d2e9467a5cde04f1be))
* better transaction categorization with ML predictions on page ([2494799](https://github.com/sousandrei/plinth/commit/2494799aa7daec1abb2e3fb94ed2eddc4cfdd5d8))
* delete account and all related data flow ([9feef95](https://github.com/sousandrei/plinth/commit/9feef950af65629a33bba2acfc857718e96e95e2))
* force sync with all peers from Spaces page ([c66a49e](https://github.com/sousandrei/plinth/commit/c66a49e67b6cd7d42944a79138f3133d95ad291c))
* full snapshot syncronization ([3c9eec6](https://github.com/sousandrei/plinth/commit/3c9eec6e46a1838b096f0a533b218788ebaba30d))
* full text search with trigram tokenizer and all fields ([400fc55](https://github.com/sousandrei/plinth/commit/400fc55bcf7f122a417dfb10c0af2613fb905ac4))
* online indicator for local network devices ([2e5a662](https://github.com/sousandrei/plinth/commit/2e5a662966d8160de2f5a6379e960c5b628ac9ba))
* refine online indicator and mDNS peer tracking ([7e419d7](https://github.com/sousandrei/plinth/commit/7e419d71128307c2c1e10f209a232855d179776f))


### Bug Fixes

* bulk edit category select z-index issue ([fb983fc](https://github.com/sousandrei/plinth/commit/fb983fc19f8e7903b0d65ce80f191200b6939a98))
* conflict on upsert trusted device query ([fa01e22](https://github.com/sousandrei/plinth/commit/fa01e228c3553ee432c0aee3e77b758c7c5c2bef))
* disable auto-correct and auto-capitalize for search input ([48c9c59](https://github.com/sousandrei/plinth/commit/48c9c592ee6a7681dd9c78f2885b3112c527edd3))
* keep spaces on seb parsers ([e5d2e75](https://github.com/sousandrei/plinth/commit/e5d2e75722eab09bd87b38ac94edab294948a8f8))
* polish transaction selection logic and category handling ([db4a07b](https://github.com/sousandrei/plinth/commit/db4a07b2e87fcff3fc88b184de65915ee79659bb))
* remove md5 from account id in SEB and Avanza parsers ([7263b81](https://github.com/sousandrei/plinth/commit/7263b81f06ed69ba55e32192ba8b13c967fec62d))

## [1.2.1](https://github.com/sousandrei/plinth/compare/plinth-v1.2.0...plinth-v1.2.1) (2026-06-22)


### Bug Fixes

* another try in release workflow to avoid duplicate artifacts ([5d58263](https://github.com/sousandrei/plinth/commit/5d582637409d671f621e7be3c32b7c1734786537))
* multi point sync with multiple devices per space ([f7cd2ff](https://github.com/sousandrei/plinth/commit/f7cd2ff4b2c0dff0de3142a354b71eb47fc5cd57))

## [1.2.0](https://github.com/sousandrei/plinth/compare/plinth-v1.1.3...plinth-v1.2.0) (2026-06-21)


### Features

* toast for up to date app ([b755ed5](https://github.com/sousandrei/plinth/commit/b755ed59ca67f16eb63ba9e89fc29f17349e4845))


### Bug Fixes

* env export on windows build ([8d30532](https://github.com/sousandrei/plinth/commit/8d30532b566bf31f2c5cce578a5b76930558cbad))

## [1.1.3](https://github.com/sousandrei/plinth/compare/plinth-v1.1.2...plinth-v1.1.3) (2026-06-21)


### Bug Fixes

* simplify ci release workflow and fix cuda builds ([5633df6](https://github.com/sousandrei/plinth/commit/5633df649d709ff4ae848d4c26d9b7c1ddff2281))

## [1.1.2](https://github.com/sousandrei/plinth/compare/plinth-v1.1.1...plinth-v1.1.2) (2026-06-21)


### Bug Fixes

* **ci:** escape $ in PowerShell command to avoid variable expansion ([0ead10b](https://github.com/sousandrei/plinth/commit/0ead10bb9555d9b85762046956afdf73883c8d4f))
* narrow files to be released in macos ([9da5355](https://github.com/sousandrei/plinth/commit/9da5355402d3810fc7e20a952e98607fd2fdb768))
* rounded corners on category chips ([63238ce](https://github.com/sousandrei/plinth/commit/63238cedcea9d8e239fc898a46614dbad7e9f891))

## [1.1.1](https://github.com/sousandrei/plinth/compare/plinth-v1.1.0...plinth-v1.1.1) (2026-06-21)


### Miscellaneous Chores

* release 1.1.1 ([778c52d](https://github.com/sousandrei/plinth/commit/778c52d7c4fc872bdc5b8999fc646bff8cf58409))

## [1.1.0](https://github.com/sousandrei/plinth/compare/plinth-v1.0.0...plinth-v1.1.0) (2026-06-21)


### Features

* toaster and updater ([227da44](https://github.com/sousandrei/plinth/commit/227da44b2892e3014131c3f915337c79674ad00e))

## 1.0.0 (2026-06-21)


### Features

* account colors ([7bf4295](https://github.com/sousandrei/plinth/commit/7bf4295bf25adb712783b099cea8b0f3d03fe0d5))
* add bulk actions for transactions ([4386ad9](https://github.com/sousandrei/plinth/commit/4386ad9944952d4dbb603317e972000a4d4bf9d2))
* Add bulk approve/categorize transactions commands and queries ([4fdd9e3](https://github.com/sousandrei/plinth/commit/4fdd9e392b50185303603eba8f366b2685e1b23d))
* add nicer logo and icon ([60b41b8](https://github.com/sousandrei/plinth/commit/60b41b8999f8ec03f7f210765dbd3b02f5715683))
* better ui around spaces ([7f87daf](https://github.com/sousandrei/plinth/commit/7f87daf6e8c48afcd33bd34b3965220d385d7335))
* demo mode and switch component ([5f98d4f](https://github.com/sousandrei/plinth/commit/5f98d4fcba9ea765bd0fef564ddcec07e53d72cd))
* import export backend support ([69fd6a7](https://github.com/sousandrei/plinth/commit/69fd6a733cd99e2f3eeb2fa42ff9d77d46a198e6))
* migrate to space based data model ([cad11fb](https://github.com/sousandrei/plinth/commit/cad11fb7ed4a14d0b0e15c49133f54d0010442c8))
* model synchronization and debounce mechanism ([65c808d](https://github.com/sousandrei/plinth/commit/65c808d43b8698fa9e58421c9c2e304d8a5ab180))
* open source project ([882e28e](https://github.com/sousandrei/plinth/commit/882e28ee49bd48db795ca4b7f39772bfaf44b9c1))
* polish user creating and onboarding ([6edd4f8](https://github.com/sousandrei/plinth/commit/6edd4f89a77056c8b809f3f43d3875d647ae020e))
* sync apply functionality with upsert and delete operations ([92f4512](https://github.com/sousandrei/plinth/commit/92f451232c0ef73c3b5ff9c36a206efb65acd274))
* sync mechanism with apply guard and payloads ([98e07ed](https://github.com/sousandrei/plinth/commit/98e07ed6dfb62497cf7790598f93d18015536c02))


### Bug Fixes

* another delete space flow and add user on joining flow ([5221412](https://github.com/sousandrei/plinth/commit/5221412bc637728d34d099e23eb45c8bc619b11a))
* better space deletion sync ([de78044](https://github.com/sousandrei/plinth/commit/de780441197c2d6fe2ad89545757561199819c34))
* better space eviction ([f55de06](https://github.com/sousandrei/plinth/commit/f55de0602b6267a6d0f6efbf29478060e21a4813))
* clean up mutex poisoning cases ([a5454c8](https://github.com/sousandrei/plinth/commit/a5454c8997f89e4b1747ba43bf914bfe67c56e3a))
* dashboard query ([207900f](https://github.com/sousandrei/plinth/commit/207900f31925c0b69cc9b422cb0eae876f58d059))
* device naming ([5402aa1](https://github.com/sousandrei/plinth/commit/5402aa111a1ebffc0f7c0c172230554c7992e921))
* evicting trusted device ([9af0b5f](https://github.com/sousandrei/plinth/commit/9af0b5f9eb4cc4b91629c46f39052ce58fdc4f9a))
* join space bug on tls and duped user ([24a6063](https://github.com/sousandrei/plinth/commit/24a606340484654f811e5f2aa71b15752f8e3a6d))
* login on space join for existing users ([4af7a6d](https://github.com/sousandrei/plinth/commit/4af7a6d80f6549e4d5b8544f94244f75410cdedf))
* show correct predictions when switching versions ([40f30dc](https://github.com/sousandrei/plinth/commit/40f30dcf79cdbebf80e2834f2ebdf981230c4402))
* simplify upload flow ([4cfe61a](https://github.com/sousandrei/plinth/commit/4cfe61af3c745fefca66452dff9f89c12f02a383))
* space deletion and import ([2edaa94](https://github.com/sousandrei/plinth/commit/2edaa949c00dca3f9ed3c9f9b655ee425d767fae))
* space deletion logic ([a07d194](https://github.com/sousandrei/plinth/commit/a07d1944b6f729d9d04e38e1ddc9998055547198))
* trusted devices and minilm banner ([e5974fa](https://github.com/sousandrei/plinth/commit/e5974fae6549ab4e847beb524ea16734f103d379))
* use transaction for upload_file ([951f4f6](https://github.com/sousandrei/plinth/commit/951f4f6a99827ce5c775c5639dccf8faf89340ab))
