{-# LANGUAGE NoImplicitPrelude #-}
-- A program's module under GHC.* defining its own foldr and (++) (ghc-lib-parser's
-- GHC.Data.List.Infinite): list comprehensions in it still mean the list ones.
module GHC.Data.Stream (Stream (..), foldr, (++), prefixes, takeS) where

import Prelude (Int, (-), Show (..))

data Stream a = Cons a (Stream a)

foldr :: (a -> b -> b) -> Stream a -> b
foldr c (Cons a as) = c a (foldr c as)

(++) :: [a] -> Stream a -> Stream a
[] ++ s = s
(x : xs) ++ s = Cons x (xs ++ s)

prefixes :: [a] -> Stream [a]
prefixes as = go [[]] where go acc = [a : p | p <- acc, a <- as] ++ go [a : p | p <- acc, a <- as]

takeS :: Int -> Stream a -> [a]
takeS 0 _ = []
takeS n (Cons a as) = a : takeS (n - 1) as
