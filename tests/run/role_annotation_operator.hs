{-# LANGUAGE RoleAnnotations #-}
module Main where

type role (Main.:+:) _ nominal
data a :+: b = Union a b deriving Show

main :: IO ()
main = print (Union True (3 :: Int))
