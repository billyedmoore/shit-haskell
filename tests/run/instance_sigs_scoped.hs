{-# LANGUAGE AllowAmbiguousTypes #-}
{-# LANGUAGE InstanceSigs #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
module Main where

import Data.Proxy

class Tagged a where
  tag :: forall b. Proxy b -> a -> String

instance Tagged Int where
  tag :: forall i. Proxy i -> Int -> String
  tag _ n = show n ++ ":" ++ show (length (proxyList @i))

proxyList :: forall x. [Proxy x]
proxyList = [Proxy @x, Proxy @x]

main :: IO ()
main = putStrLn (tag (Proxy @Bool) (5 :: Int))
