{-# OPTIONS_GHC -Wmissing-import-lists -Wno-all -Werror #-}

import Data.Maybe hiding (fromJust)

main :: IO ()
main = print (isJust (Just (3 :: Int)))
