{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ('[Int])
value = Proxy

main :: IO ()
main = pure ()
