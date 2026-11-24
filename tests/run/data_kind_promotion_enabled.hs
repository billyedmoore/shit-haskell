{-# LANGUAGE DataKinds #-}

import Data.Proxy
data D = MkD
value :: Proxy 'MkD
value = Proxy

unticked :: Proxy MkD
unticked = Proxy

main :: IO ()
main = pure ()
