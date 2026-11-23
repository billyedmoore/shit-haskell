module BootBadInlinePragma where
result :: Int
{-# INLINE result #-}
result = 1
