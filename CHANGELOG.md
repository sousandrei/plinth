# Changelog

## [1.6.0](https://github.com/sousandrei/plinth/compare/plinth-v1.5.1...plinth-v1.6.0) (2026-11-20)


### Features

* add authenticated sync enrollment transport ([0f00da1](https://github.com/sousandrei/plinth/commit/0f00da1eca6db15fc91a0b6c06cb65f8927209f8))
* add encrypted peer connectivity demo ([8040711](https://github.com/sousandrei/plinth/commit/8040711c5a303d8d9a252da4747d3b40b63e046e))
* add headless sync peer ([ca44ee3](https://github.com/sousandrei/plinth/commit/ca44ee305a9a376cb866fcc6e52d4db1b90bb373))
* add space sharing to the sync preview ([3a82c03](https://github.com/sousandrei/plinth/commit/3a82c03b2e8aad4c15258e55e9e1afe6db0fa51e))
* add transaction sync preview ([19b4023](https://github.com/sousandrei/plinth/commit/19b4023157d00444dc6a4d3230c9b0d65b077f86))
* authenticate permission consensus traffic over tls ([31f40e9](https://github.com/sousandrei/plinth/commit/31f40e96c1ddc49559adfbf2178585b65dc4f773))
* bootstrap shared spaces from seed files ([f5cf3ad](https://github.com/sousandrei/plinth/commit/f5cf3ad6c0734df85ba2cbdfe4fb8bb8ac2e6e34))
* checkpoint sync storage in the background ([40fb72e](https://github.com/sousandrei/plinth/commit/40fb72e1f4f3146949fb3df04ec4f07983dfbbd3))
* clean up local copies after confirmed space removals ([636486a](https://github.com/sousandrei/plinth/commit/636486a12a8a669157826bf98b5efbf69d4188f9))
* commit shared device removals through the permission group ([31c1af8](https://github.com/sousandrei/plinth/commit/31c1af87273c22b7cb844a34ca56b1f4e927dfc1))
* commit shared space deletions before local cleanup ([07dd35f](https://github.com/sousandrei/plinth/commit/07dd35fcb6d607ff3babe563543edae6cd458b79))
* commit shared space permissions across owner devices ([9ef3686](https://github.com/sousandrei/plinth/commit/9ef3686332a11df6ed8716d74b50658c482bf064))
* define ordered space membership changes ([d109f97](https://github.com/sousandrei/plinth/commit/d109f978ff752dae7c592a09130b4168bff48bb8))
* deliver sync removal notices on reconnect ([6d1cf2c](https://github.com/sousandrei/plinth/commit/6d1cf2cdb17bead799a9f4c099c1fed3f440eb25))
* discover nearby sync peers ([a6beb6a](https://github.com/sousandrei/plinth/commit/a6beb6adbd85fd82015b6f763eea5f08f796142f))
* form the initial permission quorum automatically ([7952648](https://github.com/sousandrei/plinth/commit/7952648d31e8002c6b8d851ffa1ed0fb4034fcf0))
* initialize permissions for locally created spaces ([86e5cea](https://github.com/sousandrei/plinth/commit/86e5cead32576a3392b377cf1335e740b1ff6705))
* join shared spaces and sync paired devices ([cd1ba7b](https://github.com/sousandrei/plinth/commit/cd1ba7bb9a3eb515c50efa086a92d8cb6c756676))
* migrate to burn from candle ([419f455](https://github.com/sousandrei/plinth/commit/419f455b20cdc2ca0a309f0db1c73c48899b7155))
* pair nearby devices from onboarding ([060cfaa](https://github.com/sousandrei/plinth/commit/060cfaab611e82e005ed0a75f5461d0c0aeacdc2))
* persist space permission consensus with openraft ([17dc50b](https://github.com/sousandrei/plinth/commit/17dc50b54bfb5a77768b6f2a827370ae4a11e2ff))
* persist sync acknowledgments by device enrollment ([71c8d36](https://github.com/sousandrei/plinth/commit/71c8d366d4a24a72ab59dfa8568dc1f4e9bf7cac))
* prototype peer sync ([06ff34f](https://github.com/sousandrei/plinth/commit/06ff34fdb0de57ee2259366ccf19674ba4901407))
* record trusted peer contacts during pairing ([d0cd986](https://github.com/sousandrei/plinth/commit/d0cd986d939a611c45db22671165c9132b5a21d7))
* resume desktop permission groups after restart ([581741f](https://github.com/sousandrei/plinth/commit/581741fa679fbc34fef0dedc32e4a36242dd9055))
* route desktop pairing through live permission replicas ([8b4db0f](https://github.com/sousandrei/plinth/commit/8b4db0f9f2f447418c1b0584372a32579b8cc29c))
* route shared invitations through the permission leader ([eead5b6](https://github.com/sousandrei/plinth/commit/eead5b601a85b333131406021c3e2f0afcd68be8))
* share and join spaces in the desktop app ([45aed13](https://github.com/sousandrei/plinth/commit/45aed139f8c89a1b802f49b36a804ba09e42e2a0))
* sync directly with peers learned from the space roster ([b76f339](https://github.com/sousandrei/plinth/commit/b76f3394b66b194605f70debd15d23ceeec35569))
* sync new bank transactions and accounts ([84f9055](https://github.com/sousandrei/plinth/commit/84f9055971f5102599e6a0681d2baa3cc4288f82))
* sync transaction edits between preview peers ([e496074](https://github.com/sousandrei/plinth/commit/e496074123425920f7deb1ef9280642c85738c96))
* transfer committed permissions during pairing ([88a2464](https://github.com/sousandrei/plinth/commit/88a2464a674e69082ac5f8b133c9543de9fd45d7))


### Bug Fixes

* confirm pairing with the joining code alone ([f56c3b5](https://github.com/sousandrei/plinth/commit/f56c3b5bb159860aa5b3fbf86098de757875fa96))
* identify nearby and paired devices by name ([e7bda08](https://github.com/sousandrei/plinth/commit/e7bda083741947ff808cefba294e25e36acb1e0e))
* keep open tables steady during sync refresh ([21dee29](https://github.com/sousandrei/plinth/commit/21dee29a637388312ed607349a36961fbeae5a67))
* refresh open views after incoming sync ([69e564c](https://github.com/sousandrei/plinth/commit/69e564ca08c09370ac8c4d9e004ed708f389aca9))
* require entering the shared space joining code ([3565b38](https://github.com/sousandrei/plinth/commit/3565b38490b4683c9a799de495a82ffb33307d33))
* show lan pairing addresses and declare macos local network access ([5f9555b](https://github.com/sousandrei/plinth/commit/5f9555ba54543e6cbdd4cdbb6555ddbcda46c60e))
* verify device keys during pairing ([a7f85aa](https://github.com/sousandrei/plinth/commit/a7f85aa9b9302d8ec374e79f7946f2c93140020c))

## [1.5.1](https://github.com/sousandrei/plinth/compare/plinth-v1.5.0...plinth-v1.5.1) (2026-07-27)


### Bug Fixes

* parsing screen units, script saving and avanza script update ([97d0113](https://github.com/sousandrei/plinth/commit/97d01137976bce365bb0989c482cec0fa0f83bf4))

## [1.5.0](https://github.com/sousandrei/plinth/compare/plinth-v1.4.0...plinth-v1.5.0) (2026-07-13)


### Features

* add first storybook integration ([a8e5a3b](https://github.com/sousandrei/plinth/commit/a8e5a3b4c459c721f34fd9a033ef1ab4711eec4f))
* add more stories for pages and subcomponents ([3a60b72](https://github.com/sousandrei/plinth/commit/3a60b721875f7af367cefa2e64ed356ec14dbbf2))
* add visual tests ([14e44a3](https://github.com/sousandrei/plinth/commit/14e44a35d522007f4930fcb30aca3162b802ad17))
* more stories ([4ba5a3b](https://github.com/sousandrei/plinth/commit/4ba5a3b44f5ee88685603700aace08edb3841e8a))
* polish dashboard components ([6e67445](https://github.com/sousandrei/plinth/commit/6e67445ba4b9d1e1db534d0ca366aa271d2d0804))


### Bug Fixes

* do not ping stale peers ([8a0a39b](https://github.com/sousandrei/plinth/commit/8a0a39bfb0ea47da7f3ba10e5d4113fec4e98fd4))
* remove base model concept and update classifier loading logic ([7d63eec](https://github.com/sousandrei/plinth/commit/7d63eecc59ef757a6f33a474a03cf93901ea34bf))

## [1.4.0](https://github.com/sousandrei/plinth/compare/plinth-v1.3.0...plinth-v1.4.0) (2026-06-29)


### Features

* better presence indication with Ping/Pong heartbeats ([3f1f25e](https://github.com/sousandrei/plinth/commit/3f1f25e51b40dc14548ffa65bfcf4d7264311601))
* more complete model_versions and sync ([36eb972](https://github.com/sousandrei/plinth/commit/36eb972e31a21dabc01211fe51cd586f8daa24cb))


### Bug Fixes

* better cash flow calculation and net worth delta display ([6f5108b](https://github.com/sousandrei/plinth/commit/6f5108b986352a40fc28e6c070a0728542b4ae32))
* better force sync ui flow ([04a0956](https://github.com/sousandrei/plinth/commit/04a095644c3a0745dfe77752e284bceb5c595f5a))
* online presence and debounce trigger ([de3293a](https://github.com/sousandrei/plinth/commit/de3293ad544641cc696e1fbea4d1855db1165620))
* sync model weights and card MD5s for divergence detection ([ce7a889](https://github.com/sousandrei/plinth/commit/ce7a889d9b870a5073418ec1da778d16c87c0ca8))
* training progress updates ([e0ebb8a](https://github.com/sousandrei/plinth/commit/e0ebb8a47d8386f1b2438fdfaaa70947624e3fdf))

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
