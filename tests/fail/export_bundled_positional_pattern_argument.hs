{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(.., value, Other)) where

data T = Here Int

pattern Other :: Int -> T
pattern Other value = Here value

value :: Int
value = 3

main :: IO ()
main = pure ()
