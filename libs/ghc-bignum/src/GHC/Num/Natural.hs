{-# LANGUAGE NoImplicitPrelude #-}
-- The Natural type lives with Integer (its conversions need both); this module re-exports it.
module GHC.Num.Natural (module GHC.Num.Integer) where

import GHC.Num.Integer
