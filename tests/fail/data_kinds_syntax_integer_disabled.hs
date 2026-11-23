{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy (12)
value = Proxy

main :: IO ()
main = pure ()
