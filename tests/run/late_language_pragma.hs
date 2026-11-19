-- A LANGUAGE pragma after the module header is ignored, as GHC ignores it (base16 has a stray
-- `{-# LANGUAGE lenientLoop #-}` after a definition).
module Main (main) where

twice :: Int -> Int
twice x = x * 2
{-# LANGUAGE noSuchExtension #-}

main :: IO ()
main = print (twice 21)
