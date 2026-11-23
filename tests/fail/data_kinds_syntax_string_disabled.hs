{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ("hello")
value = Proxy

main :: IO ()
main = pure ()
