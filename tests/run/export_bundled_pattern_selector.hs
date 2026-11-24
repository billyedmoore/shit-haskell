{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(.., field, Other)) where

data T = Here Int

pattern Other :: Int -> T
pattern Other {field} = Here field

main :: IO ()
main = print (field (Here 7))
