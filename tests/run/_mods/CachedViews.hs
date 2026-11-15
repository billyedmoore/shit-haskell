{-# LANGUAGE PatternSynonyms, ViewPatterns, TupleSections, TypeApplications #-}
module CachedViews
  ( pattern Shifted, pattern Tagged, pattern Positive, pattern Same, pattern Single, pattern Seen
  , pattern Typed, pattern Recorded, pattern Updated ) where

pattern Shifted :: Int -> Int
pattern Shifted n <- ((+ 2) -> n) where
  Shifted n = n - 2

pattern Tagged :: Int -> Int
pattern Tagged n <- ((, True) -> (n, True))

pattern Positive :: Int
pattern Positive <- ((\x -> if x > 0 then True else False) -> True)

pattern Same :: Int -> Int
pattern Same n <- (id @Int -> n)

pattern Single :: Int -> Int
pattern Single n <- ((\x -> [x]) -> [n])

privateView :: Int -> Maybe Int
privateView n = Just (n + 1)

pattern Seen :: Int -> Int
pattern Seen n <- (privateView -> Just n)

pattern Typed :: Int -> Int
pattern Typed n <- ((id :: Int -> Int) -> n)

data RecordView = RecordView { runView :: Int -> Int }

pattern Recorded :: Int -> Int
pattern Recorded n <- (runView (RecordView { runView = id }) -> n)

pattern Updated :: Int -> Int
pattern Updated n <- (runView ((RecordView id) { runView = (+ 1) }) -> n)
