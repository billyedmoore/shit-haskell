{-# LANGUAGE NoImplicitPrelude #-}
-- The module's own name as an import alias (ghc-lib-parser's GHC.Prelude): `module Lib.Prelude`
-- exports both its own definitions and that import's names.
module Lib.Prelude (module Lib.Prelude, module Data.Maybe) where

import Lib.Prelude.Basic as Lib.Prelude
import Data.Maybe (fromMaybe)

twice :: Int -> Int
twice n = n * 2
