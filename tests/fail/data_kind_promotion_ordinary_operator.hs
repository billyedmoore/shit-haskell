{-# LANGUAGE DataKinds #-}

import Data.Proxy
value :: Proxy ((+) Int Int)
value = Proxy

main :: IO ()
main = pure ()
