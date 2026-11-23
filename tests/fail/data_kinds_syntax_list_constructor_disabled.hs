{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ((:))
value = Proxy

main :: IO ()
main = pure ()
