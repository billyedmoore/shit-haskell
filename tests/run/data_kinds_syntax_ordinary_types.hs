{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ([Int], Bool)
value = Proxy

constructor :: Proxy []
constructor = Proxy

main :: IO ()
main = print ([12 :: Int], "hello", 'x')
