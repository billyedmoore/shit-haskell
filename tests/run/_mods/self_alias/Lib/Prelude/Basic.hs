{-# LANGUAGE NoImplicitPrelude #-}
-- `module X` of imports aliased X, with a hiding list.
module Lib.Prelude.Basic (module X, head) where

import Prelude as X hiding (head)
import qualified Prelude

head :: [a] -> a
head (x : _) = x
head [] = Prelude.error "Lib.Prelude.Basic.head: empty"
