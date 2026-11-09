module System.Environment (getArgs, getProgName, lookupEnv, getEnvironment) where

import Base

foreign import prim "getArgs" primGetArgs :: World -> (# World, [String] #)
foreign import prim "getProgName" primGetProgName :: World -> (# World, String #)

getArgs :: IO [String]
getArgs = IO primGetArgs

getProgName :: IO String
getProgName = IO primGetProgName

lookupEnv :: String -> IO (Maybe String)
lookupEnv _ = return Nothing

getEnvironment :: IO [(String, String)]
getEnvironment = return []
