module Data.Either (Either(..), either, lefts, rights, partitionEithers, isLeft, isRight, fromLeft, fromRight) where

import Base

lefts :: [Either a b] -> [a]
lefts xs = [x | Left x <- xs]

rights :: [Either a b] -> [b]
rights xs = [x | Right x <- xs]

partitionEithers :: [Either a b] -> ([a], [b])
partitionEithers xs = (lefts xs, rights xs)

isLeft :: Either a b -> Bool
isLeft (Left _) = True
isLeft _ = False

isRight :: Either a b -> Bool
isRight (Right _) = True
isRight _ = False

fromLeft :: a -> Either a b -> a
fromLeft _ (Left a) = a
fromLeft a _ = a

fromRight :: b -> Either a b -> b
fromRight _ (Right b) = b
fromRight b _ = b
