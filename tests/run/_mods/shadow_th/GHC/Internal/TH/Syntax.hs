-- | A home module with the name of one of ghc-internal's (as ghc-lib-parser ships its own
-- GHC.Internal.TH.Syntax): the template-haskell library still uses ghc-internal's, which exports
-- itself (`module GHC.Internal.TH.Syntax`).
module GHC.Internal.TH.Syntax (homeVersion) where

homeVersion :: String
homeVersion = "the program's own GHC.Internal.TH.Syntax"
