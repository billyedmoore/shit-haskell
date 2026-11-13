-- | A home module with the name of one of the time package's hidden modules (as time-compat
-- has); time's own modules still import theirs.
module Data.Time.Calendar.Private (pad2) where

import Data.Time (Day, toGregorian)

pad2 :: Day -> String
pad2 d = let (_, m, _) = toGregorian d in (if m < 10 then "0" else "") ++ show m
