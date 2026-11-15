{-# LANGUAGE PatternSynonyms #-}
module Main where
import CachedViews

positive :: Int -> Bool
positive Positive = True
positive _ = False

main :: IO ()
main = do
  print (case (5 :: Int) of Shifted n -> n)
  print (Shifted 7)
  print (case (9 :: Int) of Tagged n -> n)
  print (map positive [-1, 0, 2])
  print (case (11 :: Int) of Same n -> n)
  print (case (13 :: Int) of Single n -> n)
  print (case (17 :: Int) of Seen n -> n; _ -> 0)
  print (case (19 :: Int) of Typed n -> n)
  print (case (23 :: Int) of Recorded n -> n)
  print (case (29 :: Int) of Updated n -> n)
