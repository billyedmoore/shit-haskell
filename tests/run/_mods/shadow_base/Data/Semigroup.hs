-- | A home module with the name of a base module that containers imports (as ghc-lib-parser
-- ships its own GHC.Internal.* modules): containers' modules still import base's.
module Data.Semigroup (shout) where

shout :: String -> String
shout s = s ++ "!"
